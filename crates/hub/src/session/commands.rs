//! Carrying out what clients ask of a session, one at a time.

use std::sync::Arc;

use protocol::attachment::Attachment;
use protocol::client::{ApiError, ErrorCode};
use protocol::event::{ApprovalDecision, ApprovalOutcome, EventBody, NoticeLevel};
use protocol::model::{ApprovalId, AttachmentId, ClientMessageId, Seq, SetupStatus};
use protocol::prompt::Prompt;
use protocol::worker::{AgentApproval, AgentCancel, AgentCompact, AgentPrompt, AgentStop, Delivery, Request};
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use super::{Command, LiveSession, Origin};
use crate::hub::Hub;

mod compact;
mod launch;

/// What the agent is asked, once it's recorded.
enum Asking {
    Prompt(Prompt<Attachment>),
    Compact { keeping: Option<String> },
}

pub(crate) async fn run_commands(session: Arc<LiveSession>, mut commands: mpsc::Receiver<Command>) {
    while let Some(command) = commands.recv().await {
        match command {
            Command::Prompt { client_msg_id, prompt, reply } => {
                session.handle_prompt(client_msg_id, prompt, reply).await;
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
    async fn forward(&self, seq: Seq, asking: Asking) -> Result<(), ApiError> {
        let session_id = self.id.clone();
        match asking {
            Asking::Prompt(prompt) => {
                // Only now, after waiting on setup, since the worktree has to be ready for them.
                let placed = self.hub()?.place_attachments(&self.id, &self.workspace_id, &prompt.files).await?;
                self.send_to_agent(AgentPrompt { session_id, seq, prompt: prompt.with_files(placed) }).await
            }
            Asking::Compact { keeping } => self.send_to_agent(AgentCompact { session_id, asked: seq, keeping }).await,
        }
    }

    /// Sends what was recorded to the agent, starting one if it has to.
    async fn send_to_agent<R: Request<Reply = Delivery> + Clone>(&self, request: R) -> Result<(), ApiError> {
        // Twice, because the agent can exit between checking on it and sending to it.
        for _ in 0..2 {
            let link = self.ensure_agent().await?;
            if link.call(request.clone()).await? == Delivery::Delivered {
                return Ok(());
            }
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
        hub.ready_workspace(&self.workspace_id).await.map(drop)
    }

    /// Returns the prompt's place in the log, whether it had already been recorded, and the prompt with the hub's record
    /// of each file it carries.
    async fn record_prompt(
        &self,
        hub: &Arc<Hub>,
        client_msg_id: Option<ClientMessageId>,
        prompt: Prompt<AttachmentId>,
    ) -> Result<(Seq, bool, Prompt<Attachment>), ApiError> {
        let Prompt { text, files } = prompt;
        let id = self.id.clone();
        let prompt = Prompt::carrying(text, hub.blocking(move |store| store.attachments(&id, &files)).await?);
        let (seq, duplicate) = self.record_carrying(hub, client_msg_id, &prompt).await?;
        Ok((seq, duplicate, prompt))
    }

    /// Records a prompt whose files are known already, such as a queued one's, and returns its place in the log and
    /// whether it had already been recorded.
    pub(super) async fn record_carrying(
        &self,
        hub: &Arc<Hub>,
        client_msg_id: Option<ClientMessageId>,
        prompt: &Prompt<Attachment>,
    ) -> Result<(Seq, bool), ApiError> {
        self.can_be_asked(hub).await?;
        let body = EventBody::UserPrompt { client_msg_id, prompt: prompt.clone() };
        let (event, duplicate) = self.record(Origin::Hub, body, None).await?;
        if !duplicate {
            hub.title_from_prompt(&self.id, &self.workspace_id, prompt).await;
        }
        Ok((event.seq, duplicate))
    }

    async fn handle_prompt(
        &self,
        client_msg_id: Option<ClientMessageId>,
        prompt: Prompt<AttachmentId>,
        reply: oneshot::Sender<Result<(Seq, bool), ApiError>>,
    ) {
        let hub = match self.hub() {
            Ok(hub) => hub,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        let (seq, duplicate, prompt) = match self.record_prompt(&hub, client_msg_id, prompt).await {
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
            self.deliver_prompt(&hub, seq, prompt).await;
        }
    }

    pub(super) async fn deliver_prompt(&self, hub: &Hub, seq: Seq, prompt: Prompt<Attachment>) {
        self.deliver(hub, seq, Asking::Prompt(prompt)).await;
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
                    parent_tool_use_id: None,
                },
            )
            .await;
        }
    }

    async fn handle_approve(&self, approval_id: ApprovalId, decision: ApprovalDecision) -> Result<(), ApiError> {
        {
            let state = self.state.lock();
            let Some(asked_by) = state.progress.approvals.get(&approval_id) else {
                return Err(ApiError::new(
                    ErrorCode::Conflict,
                    format!("approval {approval_id} isn't waiting for an answer"),
                ));
            };
            if asked_by.is_none() || state.agent.map(|agent| agent.run) != *asked_by {
                return Err(ApiError::new(ErrorCode::Conflict, "the agent that asked has exited"));
            }
        }
        // The answers are recorded as well as passed on, in the form they came, since the agent's own words for them
        // are its adapter's to choose.
        let (outcome, answers_by_text, answers_by_position) = match &decision {
            ApprovalDecision::Allow { .. } => (ApprovalOutcome::Allowed, None, None),
            ApprovalDecision::AnswerByText { answers } => (ApprovalOutcome::Allowed, Some(answers.clone()), None),
            ApprovalDecision::AnswerByPosition { answers } => (ApprovalOutcome::Allowed, None, Some(answers.clone())),
            ApprovalDecision::Deny { .. } => (ApprovalOutcome::Denied, None, None),
        };
        let resolved = EventBody::ApprovalResolved {
            approval_id: approval_id.clone(),
            outcome,
            answers_by_text,
            answers_by_position,
            by_setting: false,
        };
        self.record(Origin::Hub, resolved, None).await?;
        self.pass_on(approval_id, decision).await
    }

    /// Passes `decision` on to the agent that asked, once it's recorded, and says in the log if it had gone.
    pub(super) async fn pass_on(&self, approval_id: ApprovalId, decision: ApprovalDecision) -> Result<(), ApiError> {
        let hub = self.hub()?;
        let link = hub
            .existing_worker(&self.workspace_id)
            .ok_or_else(|| ApiError::new(ErrorCode::Conflict, "the agent that asked has exited"))?;
        let request = AgentApproval { session_id: self.id.clone(), approval_id, decision };
        if link.call(request).await? == Delivery::Delivered {
            return Ok(());
        }
        self.record_or_log(
            Origin::Hub,
            EventBody::Notice {
                level: NoticeLevel::Warning,
                message: "The agent had already exited when this approval was answered.".to_string(),
                parent_tool_use_id: None,
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
        link.call(AgentCancel { session_id: self.id.clone() }).await.map(drop)
    }

    pub(crate) async fn handle_stop(&self) -> Result<(), ApiError> {
        let hub = self.hub()?;
        let Some(link) = hub.existing_worker(&self.workspace_id) else {
            return Ok(());
        };
        link.call(AgentStop { session_id: self.id.clone() }).await.map(drop)
    }
}
