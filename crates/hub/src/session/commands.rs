//! Carrying out what clients ask of a session, one at a time.

use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode};
use protocol::event::{ApprovalDecision, ApprovalOutcome, EventBody, ExitReason, NoticeLevel};
use protocol::model::{ApprovalId, ClientMessageId, Seq, SetupStatus, WorkspaceStatus};
use protocol::worker::{AgentSpec, WorkerRequest, WorkerResponse};
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use super::{AgentRun, Command, LiveSession, Origin};
use crate::hub::Hub;
use crate::store::Store;
use crate::worker_link::WorkerLink;

pub(crate) async fn run_commands(session: Arc<LiveSession>, mut commands: mpsc::Receiver<Command>) {
    while let Some(command) = commands.recv().await {
        match command {
            Command::Prompt { text, client_msg_id, reply } => session.handle_prompt(text, client_msg_id, reply).await,
            Command::Approve { approval_id, decision, reply } => {
                let _ = reply.send(session.handle_approve(approval_id, decision).await);
            }
            Command::Cancel { reply } => {
                let _ = reply.send(session.handle_cancel().await);
            }
            Command::Stop { reply } => {
                let _ = reply.send(session.handle_stop().await);
            }
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

    /// Picks the run before the launch and marks it current, so the new agent's first deltas aren't
    /// taken for a replaced agent's and dropped.
    async fn start_agent(
        &self,
        hub: &Hub,
        link: &WorkerLink,
        cwd: String,
        env: Vec<(String, String)>,
    ) -> Result<(), ApiError> {
        let (resume, model, permission_mode) = {
            let state = self.state.lock();
            (state.agent_opened, state.model.clone(), state.permission_mode.clone())
        };
        let asked = hub.blocking(Store::next_run).await?;
        let spec = AgentSpec {
            session_id: self.id.clone(),
            run: asked,
            agent: self.agent,
            cwd,
            agent_session_id: self.agent_session_id.clone(),
            resume,
            model,
            permission_mode,
            env,
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
            self.record_or_log(Origin::Run { run, current }, started).await;
        }
        Ok(())
    }

    async fn forward_prompt(&self, text: String) -> Result<(), ApiError> {
        // Twice, because the agent can exit between checking on it and sending to it.
        for _ in 0..2 {
            let link = self.ensure_agent().await?;
            let request = WorkerRequest::AgentPrompt { session_id: self.id.clone(), text: text.clone() };
            let WorkerResponse::AgentNotRunning = link.call(request).await? else {
                return Ok(());
            };
            let mut state = self.state.lock();
            if state.agent.is_some_and(|agent| agent.link == link.id) {
                state.agent = None;
            }
        }
        Err(ApiError::new(ErrorCode::Worker, "the agent keeps exiting before it can take the prompt"))
    }

    /// Returns the prompt's place in the log, and whether it had already been recorded.
    async fn record_prompt(
        &self,
        hub: &Hub,
        text: &str,
        client_msg_id: Option<ClientMessageId>,
    ) -> Result<(Seq, bool), ApiError> {
        if self.is_closed() {
            return Err(ApiError::new(ErrorCode::Conflict, "this chat is closed"));
        }
        if hub.workspace(&self.workspace_id).await?.status != WorkspaceStatus::Ready {
            return Err(ApiError::new(ErrorCode::Conflict, "the workspace isn't ready"));
        }
        let body = EventBody::UserPrompt { text: text.to_string(), client_msg_id: client_msg_id.clone() };
        let (event, duplicate) = self.record(Origin::Hub, body, None, client_msg_id).await?;
        if !duplicate {
            hub.title_from_prompt(&self.id, &self.workspace_id, text).await;
        }
        Ok((event.seq, duplicate))
    }

    async fn handle_prompt(
        &self,
        text: String,
        client_msg_id: Option<ClientMessageId>,
        reply: oneshot::Sender<Result<(Seq, bool), ApiError>>,
    ) {
        let hub = match self.hub() {
            Ok(hub) => hub,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        let (seq, duplicate) = match self.record_prompt(&hub, &text, client_msg_id).await {
            Ok(recorded) => recorded,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        // Recorded, so the prompt is real and the client can stop retrying. From here, failures are
        // reported in the log, where every client can see them.
        let _ = reply.send(Ok((seq, duplicate)));
        if duplicate {
            return;
        }
        hub.wait_for_setup(&self.workspace_id).await;
        if self.is_closed() {
            return;
        }
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
        if let Err(e) = self.forward_prompt(text).await {
            if self.is_closed() {
                return;
            }
            warn!(session = %self.id, "could not deliver a prompt: {e}");
            self.cancel_approvals(None).await;
            self.record_or_log(
                Origin::Hub,
                EventBody::AgentExited {
                    reason: ExitReason::StartFailed,
                    code: None,
                    signal: None,
                    stderr_tail: vec![e.message.clone()],
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
        let outcome = match decision {
            ApprovalDecision::Allow { .. } | ApprovalDecision::Answer { .. } => ApprovalOutcome::Allowed,
            ApprovalDecision::Deny { .. } => ApprovalOutcome::Denied,
        };
        let resolved = EventBody::ApprovalResolved { approval_id: approval_id.clone(), outcome };
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
