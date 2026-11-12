//! Carrying out what clients ask of a session, one at a time.

use std::sync::Arc;

use protocol::attachment::Attachment;
use protocol::client::{ApiError, ErrorCode};
use protocol::event::{ApprovalDecision, ApprovalOutcome, EventBody, ExitReason, NoticeLevel};
use protocol::model::{ApprovalId, AttachmentId, ClientMessageId, Seq, SessionStatus, SetupStatus, WorkspaceStatus};
use protocol::worker::{AgentSpec, WorkerRequest, WorkerResponse};
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use super::{AgentRun, Command, LiveSession, Origin};
use crate::hub::Hub;
use crate::store::Store;
use crate::worker_link::WorkerLink;

mod compact;

/// What the agent is asked, once it's recorded.
enum Asking {
    Prompt { text: String, attachments: Vec<Attachment> },
    Compact { keeping: Option<String> },
}

pub(crate) async fn run_commands(session: Arc<LiveSession>, mut commands: mpsc::Receiver<Command>) {
    while let Some(command) = commands.recv().await {
        match command {
            Command::Prompt { text, client_msg_id, attachments, reply } => {
                session.handle_prompt(text, client_msg_id, &attachments, reply).await;
            }
            Command::Approve { approval_id, decision, reply } => {
                let _ = reply.send(session.handle_approve(approval_id, decision).await);
            }
            Command::Cancel { reply } => {
                let _ = reply.send(session.handle_cancel().await);
            }
            Command::Compact { keeping, reply } => session.handle_compact(keeping, reply).await,
            Command::Stop { reply } => {
                let _ = reply.send(session.handle_stop().await);
            }
            Command::SendQueued => session.send_queued().await,
            Command::SendQueuedNow { client_msg_id, reply } => session.send_queued_now(client_msg_id, reply).await,
            Command::WakeAgent => session.handle_wake_agent().await,
            Command::End => break,
        }
    }
}

impl LiveSession {
    /// Finds or starts the worker and the agent, and returns the connection to use.
    async fn ensure_agent(&self) -> Result<Arc<WorkerLink>, ApiError> {
        let hub = self.hub()?;
        let link = hub.ensure_worker(&self.workspace_id).await?;
        let known = self.state.lock().agent;
        if known.is_some_and(|agent| agent.link == link.id) {
            return Ok(link);
        }
        if let Some(previous) = known {
            // The worker was replaced before its loss reached this session.
            self.agent_lost(previous.link, ExitReason::WorkerLost).await;
        }
        let workspace = hub.workspace(&self.workspace_id).await?;
        let repo = {
            let repo_id = workspace.repo_id.clone();
            hub.blocking(move |store| store.get_repo(&repo_id)).await?
        };
        let mut env = crate::hub::workspace_env(&workspace, &repo);
        env.push(("CONDUCTOR_SESSION_ID".to_string(), self.id.to_string()));
        let cwd =
            workspace.path.ok_or_else(|| ApiError::new(ErrorCode::Conflict, "the workspace has no worktree yet"))?;
        let _nothing_changes_meanwhile = self.configuring.lock().await;
        if self.is_closed() {
            return Err(ApiError::new(ErrorCode::Conflict, "this chat is closed"));
        }
        self.start_agent(&hub, &link, cwd, env).await?;
        Ok(link)
    }

    /// Has the agent started, if it's waiting on nothing but a wait, once what's asked of the session before this is
    /// done.
    pub(crate) fn wake_agent(&self) {
        if let Err(e) = self.commands.try_send(Command::WakeAgent) {
            warn!(session = %self.id, "couldn't ask for the agent to be started: {e}");
        }
    }

    /// Starts the agent for a chat that's idle or at work only in the background, with its queue not held. It needs no
    /// prompt: a CLI started again lists its tasks again and asks after them, and starts a turn by itself once one has
    /// ended, as `docs/claude-cli.md` says. One that was cut off, or held for the person, is left for them, and hears
    /// how its wait ended with their next prompt, since its CLI lists its tasks again then too, so the wait comes off
    /// its list meanwhile, rather than leave it at work on a wait that's over.
    async fn handle_wake_agent(&self) {
        let Ok(hub) = self.hub() else { return };
        let waiting_on_nothing = {
            let state = self.state.lock();
            matches!(state.status, SessionStatus::Idle | SessionStatus::Background) && !state.queue.is_held()
        };
        if self.agent_running() {
            return;
        }
        if !waiting_on_nothing || self.can_be_asked(&hub).await.is_err() {
            if let Err(e) = self.unlist_waits().await {
                warn!(session = %self.id, "couldn't take a wait that's over off the chat's list: {e}");
            }
            return;
        }
        if let Err(e) = self.ensure_agent().await {
            warn!(session = %self.id, "couldn't start the agent to hear how its wait ended: {e}");
            self.record_start_failed(&e).await;
        }
    }

    /// Tells every client the agent couldn't be started, or reached, for what was asked of it, and ends what it had
    /// asked approval for, since it won't hear the answer.
    async fn record_start_failed(&self, e: &ApiError) {
        self.cancel_approvals(None).await;
        let failed = EventBody::AgentExited {
            reason: ExitReason::StartFailed,
            code: None,
            signal: None,
            stderr_tail: vec![e.message.clone()],
        };
        self.record_or_log(Origin::Hub, failed).await;
    }

    /// Picks the run before the launch and marks it current, so the new agent's first deltas aren't
    /// taken for a replaced agent's and dropped.
    async fn start_agent(
        &self,
        hub: &Hub,
        link: &WorkerLink,
        cwd: String,
        env: Vec<(String, String)>,
    ) -> Result<(), ApiError> {
        let (resume, settings) = {
            let state = self.state.lock();
            (state.agent_opened, state.settings.clone())
        };
        let asked = hub.blocking(Store::next_run).await?;
        let tools_token = hub.tools_for(&self.workspace_id, &self.id, link).await;
        let spec = AgentSpec {
            session_id: self.id.clone(),
            run: asked,
            agent: self.agent,
            cwd,
            agent_session_id: self.agent_session_id.clone(),
            resume,
            settings,
            env,
            tools_token,
        };
        let starting = AgentRun { link: link.id, run: asked };
        self.state.lock().agent = Some(starting);
        let reply = link.call(WorkerRequest::AgentStart(spec)).await;
        let Ok(WorkerResponse::AgentStarted { pid, run, already_running }) = reply else {
            let mut state = self.state.lock();
            if state.agent == Some(starting) {
                state.agent = None;
            }
            return Err(match reply {
                Err(e) => e,
                Ok(reply) => ApiError::new(ErrorCode::Worker, format!("unexpected reply: {reply:?}")),
            });
        };
        let current = {
            let mut state = self.state.lock();
            // An agent that was already running keeps the run it started with. A new one can exit before
            // this reply arrives, and then its exit has already been recorded, so it isn't made current
            // again.
            let current = already_running || state.agent == Some(starting);
            if current {
                state.agent = Some(AgentRun { link: link.id, run });
            }
            current
        };
        if !already_running {
            // Whatever is still open was asked by a process that no longer exists.
            self.cancel_approvals(None).await;
            let started = EventBody::AgentStarted { pid, resumed: resume };
            self.record_or_log(Origin::Run { run, current, numbered: None }, started).await;
        }
        Ok(())
    }

    async fn forward(&self, seq: Seq, asking: Asking) -> Result<(), ApiError> {
        let session_id = self.id.clone();
        let request = match asking {
            Asking::Prompt { text, attachments } => {
                // Only now, after waiting on setup, since the worktree has to be ready for them.
                let attachments = self.hub()?.place_attachments(&self.id, &self.workspace_id, &attachments).await?;
                WorkerRequest::AgentPrompt { session_id, prompt: seq, text, attachments }
            }
            Asking::Compact { keeping } => WorkerRequest::AgentCompact { session_id, asked: seq, keeping },
        };
        // Twice, because the agent can exit between checking on it and sending to it.
        for _ in 0..2 {
            let link = self.ensure_agent().await?;
            let WorkerResponse::AgentNotRunning = link.call(request.clone()).await? else {
                return Ok(());
            };
            let mut state = self.state.lock();
            if state.agent.is_some_and(|agent| agent.link == link.id) {
                state.agent = None;
            }
        }
        Err(ApiError::new(ErrorCode::Worker, "the agent keeps exiting before it can be asked"))
    }

    /// Whether the agent can be asked anything: its chat is open, and its workspace is ready.
    async fn can_be_asked(&self, hub: &Hub) -> Result<(), ApiError> {
        if self.is_closed() {
            return Err(ApiError::new(ErrorCode::Conflict, "this chat is closed"));
        }
        if hub.workspace(&self.workspace_id).await?.status != WorkspaceStatus::Ready {
            return Err(ApiError::new(ErrorCode::Conflict, "the workspace isn't ready"));
        }
        Ok(())
    }

    /// Returns the prompt's place in the log, whether it had already been recorded, and what it carries.
    async fn record_prompt(
        &self,
        hub: &Arc<Hub>,
        text: &str,
        client_msg_id: Option<ClientMessageId>,
        attachments: &[AttachmentId],
    ) -> Result<(Seq, bool, Vec<Attachment>), ApiError> {
        let (id, ids) = (self.id.clone(), attachments.to_vec());
        let attachments = hub.blocking(move |store| store.attachments(&id, &ids)).await?;
        let (seq, duplicate) = self.record_carrying(hub, text, client_msg_id, &attachments).await?;
        Ok((seq, duplicate, attachments))
    }

    /// Records a prompt whose files are known already, such as a queued one's, and returns its place in the log and
    /// whether it had already been recorded.
    pub(super) async fn record_carrying(
        &self,
        hub: &Arc<Hub>,
        text: &str,
        client_msg_id: Option<ClientMessageId>,
        attachments: &[Attachment],
    ) -> Result<(Seq, bool), ApiError> {
        self.can_be_asked(hub).await?;
        let body = EventBody::UserPrompt {
            text: text.to_string(),
            client_msg_id: client_msg_id.clone(),
            attachments: attachments.to_vec(),
        };
        let (event, duplicate) = self.record(Origin::Hub, body, None, client_msg_id).await?;
        if !duplicate {
            hub.title_from_prompt(&self.id, &self.workspace_id, text, attachments).await;
        }
        Ok((event.seq, duplicate))
    }

    async fn handle_prompt(
        &self,
        text: String,
        client_msg_id: Option<ClientMessageId>,
        attachments: &[AttachmentId],
        reply: oneshot::Sender<Result<(Seq, bool), ApiError>>,
    ) {
        let hub = match self.hub() {
            Ok(hub) => hub,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        let (seq, duplicate, attachments) = match self.record_prompt(&hub, &text, client_msg_id, attachments).await {
            Ok(recorded) => recorded,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        // Recorded, so the prompt is real and the client can stop retrying. From here, failures are
        // reported in the log, where every client can see them.
        let _ = reply.send(Ok((seq, duplicate)));
        if !duplicate {
            self.deliver_prompt(&hub, seq, text, attachments).await;
        }
    }

    pub(super) async fn deliver_prompt(&self, hub: &Hub, seq: Seq, text: String, attachments: Vec<Attachment>) {
        self.deliver(hub, seq, Asking::Prompt { text, attachments }).await;
    }

    /// Gets what was recorded at `seq` to the agent, starting one if it has to. Failures go in the log, and so does
    /// what never reaches an agent, since no agent will say so.
    async fn deliver(&self, hub: &Hub, seq: Seq, asking: Asking) {
        let dropped = EventBody::PromptDropped { prompt: seq };
        hub.wait_for_setup(&self.workspace_id).await;
        if self.is_closed() {
            return self.record_or_log(Origin::Hub, dropped).await;
        }
        self.warn_if_setup_failed(hub).await;
        if let Err(e) = self.forward(seq, asking).await {
            self.record_or_log(Origin::Hub, dropped).await;
            if self.is_closed() {
                return;
            }
            warn!(session = %self.id, "could not deliver what was asked of the agent: {e}");
            self.record_start_failed(&e).await;
        }
    }

    /// Once per session, since every prompt after the first would say the same.
    async fn warn_if_setup_failed(&self, hub: &Hub) {
        if let Ok(workspace) = hub.workspace(&self.workspace_id).await
            && let SetupStatus::Failed { .. } = workspace.setup
            && !self.warned_about_setup.swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            self.record_or_log(
                Origin::Hub,
                EventBody::Notice {
                    level: NoticeLevel::Warning,
                    message: "This workspace's setup didn't finish, so the agent may find things missing.".to_string(),
                },
            )
            .await;
        }
    }

    async fn handle_approve(&self, approval_id: ApprovalId, decision: ApprovalDecision) -> Result<(), ApiError> {
        {
            let state = self.state.lock();
            let Some(asked_by) = state.pending_approvals.get(&approval_id) else {
                return Err(ApiError::new(
                    ErrorCode::Conflict,
                    format!("approval {approval_id} isn't waiting for an answer"),
                ));
            };
            if asked_by.is_none() || state.agent.map(|agent| agent.run) != *asked_by {
                return Err(ApiError::new(ErrorCode::Conflict, "the agent that asked has exited"));
            }
        }
        // The answers are recorded as well as passed on, since the agent's own words for them are its to choose.
        let (outcome, answers) = match &decision {
            ApprovalDecision::Allow { .. } => (ApprovalOutcome::Allowed, None),
            ApprovalDecision::Answer { answers } => (ApprovalOutcome::Allowed, Some(answers.clone())),
            ApprovalDecision::Deny { .. } => (ApprovalOutcome::Denied, None),
        };
        let resolved = EventBody::ApprovalResolved { approval_id: approval_id.clone(), outcome, answers };
        self.record(Origin::Hub, resolved, None, None).await?;
        let hub = self.hub()?;
        let link = hub
            .existing_worker(&self.workspace_id)
            .ok_or_else(|| ApiError::new(ErrorCode::Conflict, "the agent that asked has exited"))?;
        let request = WorkerRequest::AgentApproval { session_id: self.id.clone(), approval_id, decision };
        let WorkerResponse::AgentNotRunning = link.call(request).await? else {
            return Ok(());
        };
        self.record_or_log(
            Origin::Hub,
            EventBody::Notice {
                level: NoticeLevel::Warning,
                message: "The agent had already exited when this approval was answered.".to_string(),
            },
        )
        .await;
        Err(ApiError::new(ErrorCode::Conflict, "the agent has exited"))
    }

    /// Only touches a worker that's already up. There's nothing to cancel without one.
    async fn handle_cancel(&self) -> Result<(), ApiError> {
        let hub = self.hub()?;
        let Some(link) = hub.existing_worker(&self.workspace_id) else {
            return Ok(());
        };
        link.call(WorkerRequest::AgentCancel { session_id: self.id.clone() }).await.map(|_| ())
    }

    pub(crate) async fn handle_stop(&self) -> Result<(), ApiError> {
        let hub = self.hub()?;
        let Some(link) = hub.existing_worker(&self.workspace_id) else {
            return Ok(());
        };
        link.call(WorkerRequest::AgentStop { session_id: self.id.clone() }).await.map(|_| ())
    }
}
