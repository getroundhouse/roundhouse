use super::super::controls::Askable;
use super::super::conversation::Sending;
use super::super::decisions::refusal;
use super::super::record::Admitted;
use super::super::{Command, LiveSession, LiveState, Origin};
use super::Outgoing;
use super::after_the_turn::Sendable;
use crate::hub::Hub;
use protocol::attachment::Attachment;
use protocol::capability::Control;
use protocol::capability::approvals::Decision;
use protocol::client::{ApiError, ErrorCode};
use protocol::event::{EventBody, NoticeLevel};
use protocol::model::{ApprovalId, AttachmentId, ClientDevice, ClientMessageId, Seq, SetupStatus};
use protocol::prompt::{Prompt, PromptFrom};
use protocol::worker::{AgentStop, Delivery};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

/// What the agent is asked, once it's recorded.
pub(super) enum Asking {
    Prompt(Prompt<Attachment>),
    Compact { keeping: Option<String> },
}

/// What the agent's run did with what it was sent.
pub(super) enum Sent {
    Took,
    /// It won't take a prompt while its turn runs, so it took nothing, as [`Delivery::AfterTheTurn`] says.
    AfterTheTurn,
}

pub(crate) async fn run_commands(session: Arc<LiveSession>, mut commands: mpsc::Receiver<Command>) {
    while let Some(command) = commands.recv().await {
        match command {
            Command::Prompt { client_msg_id, prompt, device, reply } => {
                session.handle_prompt(client_msg_id, prompt, device, reply).await;
            }
            Command::Decide { approval_id, decision, reply } => {
                let _ = reply.send(session.handle_decide(approval_id, decision).await);
            }
            // After what was handed on before it, as they were when the command task did it all.
            Command::Cancel { reply } => session.hand_over(Outgoing::Cancel(reply)),
            Command::Compact { keeping, reply } => session.handle_compact(keeping, reply).await,
            Command::ReloadToolServer { server, reply } => {
                let _ = reply.send(session.reload_tool_server(&server).await);
            }
            Command::Stop { reply } => session.hand_over(Outgoing::Stop(reply)),
            Command::SendQueued => session.send_queued().await,
            Command::SendQueuedNow { client_msg_id, reply } => session.send_queued_now(client_msg_id, reply).await,
            // What reaches the agent is the delivery task's, after whatever was handed on before it.
            Command::WakeAgent => session.hand_over(Outgoing::WakeAgent),
            Command::TellTheMode => session.hand_over(Outgoing::TellTheMode),
            Command::ReconnectToolServers => session.hand_over(Outgoing::ReconnectToolServers),
            Command::SendAfterTheTurn => session.hand_over(Outgoing::AfterTheTurn),
            Command::End => break,
        }
    }
}

impl LiveSession {
    pub(super) async fn forward(&self, seq: Seq, asking: Asking) -> Result<(), ApiError> {
        match asking {
            Asking::Prompt(prompt) => {
                // Only now, after waiting on setup, since the worktree has to be ready for them.
                let placed = self.hub()?.place_attachments(&self.id, &self.workspace_id, &prompt.files).await?;
                let prompt = prompt.with_files(placed);
                let extras = protocol::capability::prompts::Extras::default();
                self.send_prompt(Sendable { seq, prompt, extras }).await
            }
            Asking::Compact { keeping } => match self.send_to_agent(Control::Compact { asked: seq, keeping }).await? {
                Sent::Took => Ok(()),
                Sent::AfterTheTurn => {
                    Err(ApiError::new(ErrorCode::Worker, "the agent won't compact while its turn runs"))
                }
            },
        }
    }

    /// Sends what was recorded to the agent, starting one if it has to, and says what its run did with it.
    pub(super) async fn send_to_agent(&self, control: Control) -> Result<Sent, ApiError> {
        // Twice, because the agent can exit between checking on it and sending to it.
        for _ in 0..2 {
            let link = self.ensure_agent().await?;
            match self.control_agent(&link, control.clone()).await? {
                Delivery::Delivered => return Ok(Sent::Took),
                Delivery::AfterTheTurn => return Ok(Sent::AfterTheTurn),
                Delivery::AgentNotRunning => {}
            }
            let mut state = self.state.lock();
            if state.agent.is_some_and(|agent| agent.link == link.id) {
                state.agent = None;
            }
        }
        Err(ApiError::new(ErrorCode::Worker, "the agent keeps exiting before it can be asked"))
    }

    /// Whether the agent can be asked anything: its chat is open, and its workspace is ready.
    pub(super) async fn can_be_asked(&self, hub: &Hub) -> Result<(), ApiError> {
        if self.is_closed() {
            return Err(ApiError::new(ErrorCode::Conflict, "this chat is closed"));
        }
        hub.ready_workspace(&self.workspace_id).await.map(drop)
    }

    /// Returns the prompt's place in the log, and the prompt to deliver, as [`LiveSession::record_carrying`] does, with
    /// the hub's record of each file it carries. The person sent it from `device`, as [`Sending::Sent`] says.
    pub(super) async fn record_prompt(
        &self,
        hub: &Arc<Hub>,
        client_msg_id: Option<ClientMessageId>,
        prompt: Prompt<AttachmentId>,
        device: Option<&ClientDevice>,
    ) -> Result<(Seq, Option<Prompt<Attachment>>), ApiError> {
        let id = self.id.clone();
        let asked = prompt.files.clone();
        let prompt = prompt.with_files(hub.blocking(move |store| store.attachments(&id, &asked)).await?);
        let sent = Sending::Sent { device };
        self.record_carrying(hub, client_msg_id, prompt, PromptFrom::Person, sent).await
    }

    /// Records a prompt whose files are known already, such as a queued one's, and returns its place in the log, and
    /// the prompt as it was recorded, to deliver, or none if it had been recorded already. `from` says who wrote it,
    /// and only the person's names an untitled chat. `sending` says how it came, which decides what it does to
    /// conversation mode, and whether it goes as said out loud.
    pub(in super::super) async fn record_carrying(
        &self,
        hub: &Arc<Hub>,
        client_msg_id: Option<ClientMessageId>,
        prompt: Prompt<Attachment>,
        from: PromptFrom,
        sending: Sending<'_>,
    ) -> Result<(Seq, Option<Prompt<Attachment>>), ApiError> {
        self.can_be_asked(hub).await?;
        let person = from.is_person();
        let (event, duplicate, prompt) = {
            let _in_order = self.recording.lock().await;
            // Decided in order with what's recorded, which is all that turns the mode on or off.
            let admitted = sending.admit(&self.state.lock().interaction, prompt);
            let (prompt, interaction) = match admitted {
                Ok(admitted) => admitted,
                Err(refusal) => return self.sent_before(hub, client_msg_id, refusal).await,
            };
            let body = EventBody::UserPrompt { client_msg_id, prompt: prompt.clone(), from };
            let admitted = Admitted { interaction, ..Admitted::default() };
            let (event, duplicate) = self.record_in_order(Origin::Hub, body, None, |_| Ok(admitted)).await?;
            (event, duplicate, prompt)
        };
        if duplicate {
            return Ok((event.seq, None));
        }
        if person {
            hub.title_from_prompt(&self.id, &self.workspace_id, &prompt).await;
        }
        Ok((event.seq, Some(prompt)))
    }

    /// Where a prompt that can't be recorded now, for `refusal`, was recorded, if it was sent before under
    /// `client_msg_id`, so a request sent again gets what the first one got whatever has changed since, as
    /// [`LiveSession::record_unless`] says of a prompt recorded twice, and nothing is delivered again.
    async fn sent_before(
        &self,
        hub: &Hub,
        client_msg_id: Option<ClientMessageId>,
        refusal: ApiError,
    ) -> Result<(Seq, Option<Prompt<Attachment>>), ApiError> {
        let Some(client_msg_id) = client_msg_id else { return Err(refusal) };
        let id = self.id.clone();
        let sent = hub.blocking(move |store| store.sent_at(&id, &client_msg_id)).await?;
        sent.map(|seq| (seq, None)).ok_or(refusal)
    }

    pub(super) async fn handle_prompt(
        &self,
        client_msg_id: Option<ClientMessageId>,
        prompt: Prompt<AttachmentId>,
        device: Option<ClientDevice>,
        reply: oneshot::Sender<Result<(Seq, bool), ApiError>>,
    ) {
        let hub = match self.hub() {
            Ok(hub) => hub,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        let (seq, prompt) = match self.record_prompt(&hub, client_msg_id, prompt, device.as_ref()).await {
            Ok(recorded) => recorded,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        // Recorded, so the prompt is real and the client can stop retrying. From here, failures are
        // reported in the log, where every client can see them.
        let _ = reply.send(Ok((seq, prompt.is_none())));
        if prompt.is_some() {
            self.hand_over(Outgoing::Deliver { seq });
        }
    }

    /// Gets what was recorded at `seq` to the agent, starting one if it has to. Failures go in the log, and so does
    /// what never reaches an agent, since no agent will say so. Only the delivery task does, once setup has ended and
    /// the worker is connected.
    pub(super) async fn deliver(&self, hub: &Hub, seq: Seq, asking: Asking) {
        let dropped = EventBody::PromptDropped { prompt: seq };
        if self.is_closed() {
            return self.record_or_log(Origin::Hub, dropped).await;
        }
        self.warn_if_setup_failed(hub).await;
        if let Err(e) = self.forward(seq, asking).await {
            self.not_delivered(seq, e).await;
        }
    }

    /// Says in the log that what was recorded at `seq` never reached an agent, and why, `e`.
    pub(super) async fn not_delivered(&self, seq: Seq, e: ApiError) {
        self.record_or_log(Origin::Hub, EventBody::PromptDropped { prompt: seq }).await;
        if self.is_closed() {
            return;
        }
        warn!(
            name: "session.request_undelivered",
            session_id = %self.id,
            error = %e,
            "Couldn't deliver what was asked of the agent"
        );
        if self.agent_running() {
            self.record_or_log(
                Origin::Hub,
                EventBody::Notice { level: NoticeLevel::Error, message: e.message, subagent: None },
            )
            .await;
        } else {
            self.record_start_failed(&e).await;
        }
    }

    /// Once per session, since every prompt after the first would say the same.
    pub(super) async fn warn_if_setup_failed(&self, hub: &Hub) {
        if let Ok(workspace) = hub.workspace(&self.workspace_id).await
            && let SetupStatus::Failed { .. } = workspace.setup
            && !self.warned_about_setup.swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            self.record_or_log(
                Origin::Hub,
                EventBody::Notice {
                    level: NoticeLevel::Warning,
                    message: "This workspace's setup didn't finish, so the agent may find things missing.".to_string(),
                    subagent: None,
                },
            )
            .await;
        }
    }

    pub(super) async fn handle_decide(&self, approval_id: ApprovalId, decision: Decision) -> Result<(), ApiError> {
        let request = self.approval_request(&approval_id).await?;
        // Recorded without what's typed in answer to a secret question, which reaches the agent and nothing else.
        let resolved = EventBody::ApprovalResolved {
            approval_id: approval_id.clone(),
            decision: Some(Box::new(decision.without_secrets(&request))),
            by_setting: false,
        };
        // Whether it's still waiting, and whether the agent that asked can take this decision, are asked as it's
        // recorded, in order with everything recorded before it, since a client can read the request in the log before
        // the session has taken it in. Both are asked before the log says the decision was made.
        let refused = |state: &LiveState| {
            let Some(asked_by) = state.progress.approvals.get(&approval_id) else {
                return Some(ApiError::new(
                    ErrorCode::Conflict,
                    format!("approval {approval_id} isn't waiting for an answer"),
                ));
            };
            if asked_by.is_none() || state.agent.map(|agent| agent.run) != *asked_by {
                return Some(ApiError::new(ErrorCode::Conflict, "the agent that asked has exited"));
            }
            refusal(&request, &decision, state.run_claims())
        };
        self.record_unless(Origin::Hub, resolved, None, refused).await?;
        self.pass_on(approval_id, decision).await
    }

    /// Passes `decision` on the request `approval` to the agent that asked, once it's recorded, and says in the log if it
    /// had gone.
    pub(in super::super) async fn pass_on(&self, approval: ApprovalId, decision: Decision) -> Result<(), ApiError> {
        let hub = self.hub()?;
        let link = hub
            .existing_worker(&self.workspace_id)
            .ok_or_else(|| ApiError::new(ErrorCode::Conflict, "the agent that asked has exited"))?;
        let delivered = match self.askable()? {
            Askable::Claimed(claims) => {
                self.control_claimed(&link, Control::Decide { approval, decision }, &claims).await?
            }
            Askable::NotRunning => Delivery::AgentNotRunning,
        };
        if delivered == Delivery::Delivered {
            return Ok(());
        }
        self.record_or_log(
            Origin::Hub,
            EventBody::Notice {
                level: NoticeLevel::Warning,
                message: "The agent had already exited when this approval was answered.".to_string(),
                subagent: None,
            },
        )
        .await;
        Err(ApiError::new(ErrorCode::Conflict, "the agent has exited"))
    }

    /// Only touches a worker that's already up. There's nothing to cancel without one.
    pub(super) async fn handle_cancel(&self) -> Result<(), ApiError> {
        let hub = self.hub()?;
        let Some(link) = hub.existing_worker(&self.workspace_id) else {
            return Ok(());
        };
        self.control_agent(&link, Control::Cancel).await.map(drop)
    }

    pub(crate) async fn handle_stop(&self) -> Result<(), ApiError> {
        let hub = self.hub()?;
        let Some(link) = hub.existing_worker(&self.workspace_id) else {
            return Ok(());
        };
        link.call(AgentStop { session_id: self.id.clone() }).await.map(drop)
    }
}
