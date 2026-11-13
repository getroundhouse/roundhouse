use super::super::{Command, LiveSession, LiveState};
use crate::hub::Hub;
use crate::store::{Changed, Queued, Queuing, Reading};
use protocol::attachment::Attachment;
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::event::{EventBody, TurnOutcome};
use protocol::model::{AttachmentId, ClientMessageId, QueuedPrompt, Seq, Session, SessionStatus};
use protocol::prompt::{Prompt, PromptProblem};
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::{error, warn};

/// A recorded prompt to get to the agent, and where it's recorded.
type Delivery = (Seq, Prompt<Attachment>);

/// What the log says about whether the agent can take a queued prompt as a turn of its own.
#[derive(Debug, Default)]
pub(in super::super) struct QueueWatch {
    /// Recorded prompts the agent hasn't taken or dropped, oldest first, as the store keeps them. Until it has taken
    /// every one, a queued prompt could fold into the turn one of them starts.
    pub(super) untaken: Vec<Seq>,
    /// The queue waits for the person, as `Session::queue_held` says.
    pub(super) held: bool,
}

impl QueueWatch {
    /// For a session as the store has it. The hub has already dropped every prompt its last run's agents had.
    pub(in super::super) fn starting(held: bool) -> Self {
        Self { held, ..Self::default() }
    }

    /// Whether the queue waits for the person.
    pub(in super::super) fn is_held(&self) -> bool {
        self.held
    }

    /// Whether the agent can take a queued prompt as a turn of its own, in a session with this status. One that went
    /// away mid-turn can, once the person lets the queue go on, since sending a prompt resumes the conversation.
    pub(in super::super) fn ready(&self, status: SessionStatus, closed: bool) -> bool {
        status.is_done() && self.untaken.is_empty() && !self.held && !closed
    }

    /// Whether the queue is held after an event for the session that takes its status `from` one `to` another, if
    /// that changes. The agent stopped, went away or failed its turn, and it's for the person to say whether to go
    /// on, since the next prompt could fail as the turn did, as on a usage limit. A prompt the person sends lets it go
    /// on once its turn ends.
    pub(in super::super) fn held_after(
        &self,
        body: &EventBody,
        from: SessionStatus,
        to: SessionStatus,
    ) -> Option<bool> {
        let stopped = matches!(body, EventBody::TurnEnded { outcome: TurnOutcome::Cancelled | TurnOutcome::Error, .. });
        let went_away = to == SessionStatus::Interrupted && from != SessionStatus::Interrupted;
        let held = if matches!(body, EventBody::UserPrompt { .. }) { false } else { self.held || stopped || went_away };
        (held != self.held).then_some(held)
    }

    pub(in super::super) fn hold(&mut self, held: bool) {
        self.held = held;
    }

    /// Whether an `idle` comes from before a prompt the agent has yet to take, so the agent isn't idle after all.
    pub(in super::super) fn idle_is_stale(&self) -> bool {
        !self.untaken.is_empty()
    }

    pub(in super::super) fn untaken(&self) -> &[Seq] {
        &self.untaken
    }

    pub(in super::super) fn set_untaken(&mut self, untaken: Vec<Seq>) {
        self.untaken = untaken;
    }
}

impl LiveSession {
    /// Queues a prompt the hub has checked can go. Each of its files has to be one of the session's.
    pub(crate) async fn queue(
        &self,
        client_msg_id: ClientMessageId,
        prompt: Prompt<AttachmentId>,
    ) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        let id = self.id.clone();
        let queued = {
            let _not_while_sending = self.queue_lock.lock().await;
            hub.blocking(move |store| {
                let Prompt { text, files } = prompt;
                let prompt = Prompt::carrying(text, store.attachments(&id, &files)?);
                store.queue_prompt(&id, &QueuedPrompt { client_msg_id, prompt })
            })
            .await?
        };
        let Queued::Done(queuing) = queued else {
            return Err(ApiError::new(ErrorCode::Conflict, "that prompt has already been sent"));
        };
        let Queuing { reading, account } = *queuing;
        // Queuing it moved when the person last did something in its workspace, if it changed the queue, and maybe
        // when its account was last used.
        let moved = reading.changed;
        let session = self.changed_queue(reading);
        if moved {
            hub.announce_workspace(&self.workspace_id).await;
        }
        if let Some(account) = account {
            hub.notify(HubNotice::Account(account));
        }
        // The turn may have ended while this was on its way.
        if self.ready_for_queue() {
            self.wake_queue();
        }
        Ok(session)
    }

    /// Changes the text of a queued prompt, unless it's too late to.
    pub(crate) async fn change_queued(
        &self,
        client_msg_id: ClientMessageId,
        text: String,
    ) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        let id = self.id.clone();
        let changed = {
            let _not_while_sending = self.queue_lock.lock().await;
            hub.blocking(move |store| store.change_queued(&id, &client_msg_id, &text)).await?
        };
        match changed {
            Changed::Done(reading) => Ok(self.changed_queue(*reading)),
            Changed::AlreadySent => Err(ApiError::new(ErrorCode::Conflict, "that prompt has already been sent")),
            Changed::NotQueued => Err(ApiError::new(ErrorCode::NotFound, "that prompt isn't queued")),
            Changed::WouldBeEmpty => Err(PromptProblem::Empty.into()),
        }
    }

    pub(crate) async fn unqueue(&self, client_msg_id: ClientMessageId) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        let id = self.id.clone();
        let reading = {
            let _not_while_sending = self.queue_lock.lock().await;
            hub.blocking(move |store| store.unqueue_prompt(&id, &client_msg_id)).await?
        };
        Ok(self.changed_queue(reading))
    }

    pub(in super::super) fn ready_for_queue(&self) -> bool {
        self.state.lock().ready_for_queue()
    }

    /// Asks the command task to send the first queued prompt. It looks again whether the agent is ready when it gets
    /// there, so asking when it's no longer ready, or twice, does nothing.
    pub(in super::super) fn wake_queue(&self) {
        if let Err(e) = self.commands.try_send(Command::SendQueued) {
            warn!(session = %self.id, "couldn't ask for the queue to be sent: {e}");
        }
    }

    /// Sends the first queued prompt, if the agent is ready for it.
    pub(in super::super) async fn send_queued(&self) {
        let Ok(hub) = self.hub() else { return };
        let to_deliver = {
            let _no_changes_meanwhile = self.queue_lock.lock().await;
            if !self.ready_for_queue() {
                return;
            }
            let id = self.id.clone();
            let next = match hub.blocking(move |store| store.first_queued(&id)).await {
                Ok(Some(next)) => next,
                Ok(None) => return,
                Err(e) => return error!(session = %self.id, "couldn't read the queue: {e}"),
            };
            match self.send_from_queue(&hub, next).await {
                Ok(to_deliver) => to_deliver,
                Err(e) => return warn!(session = %self.id, "couldn't send a queued prompt: {}", e.message),
            }
        };
        if let Some((seq, prompt)) = to_deliver {
            self.deliver_prompt(&hub, seq, prompt).await;
        }
    }

    /// Sends the queued prompt with this ID now, whether or not the agent is ready for it: into the turn it's working on,
    /// which steers it, or as a turn of its own. Answers once the prompt is recorded, as a prompt does.
    pub(in super::super) async fn send_queued_now(
        &self,
        client_msg_id: ClientMessageId,
        reply: oneshot::Sender<Result<Session, ApiError>>,
    ) {
        let hub = match self.hub() {
            Ok(hub) => hub,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        let sent = {
            let _no_changes_meanwhile = self.queue_lock.lock().await;
            let (id, wanted) = (self.id.clone(), client_msg_id);
            match hub.blocking(move |store| store.queued_prompt(&id, &wanted)).await {
                Ok(Some(prompt)) => self.send_from_queue(&hub, prompt).await,
                // Sent already, or taken out.
                Ok(None) => Ok(None),
                Err(e) => Err(e),
            }
        };
        let to_deliver = match sent {
            Ok(to_deliver) => to_deliver,
            Err(e) => {
                let _ = reply.send(Err(e));
                return;
            }
        };
        let id = self.id.clone();
        let _ = reply.send(hub.blocking(move |store| store.get_session(&id)).await.map(|stored| stored.session));
        if let Some((seq, prompt)) = to_deliver {
            self.deliver_prompt(&hub, seq, prompt).await;
        }
    }

    /// Records a queued prompt as a prompt, under its own ID, which takes it out of the queue in the same commit. The
    /// caller holds `queue_lock`, so a change can't land between reading the prompt and recording it, and be lost.
    /// Returns what to deliver, which is nothing if the prompt was recorded already.
    pub(super) async fn send_from_queue(
        &self,
        hub: &Arc<Hub>,
        prompt: QueuedPrompt,
    ) -> Result<Option<Delivery>, ApiError> {
        let QueuedPrompt { client_msg_id, prompt } = prompt;
        let (seq, duplicate) = self.record_carrying(hub, Some(client_msg_id), &prompt).await?;
        Ok((!duplicate).then_some((seq, prompt)))
    }

    /// Tells everyone following the feed, if the queue changed.
    pub(super) fn changed_queue(&self, reading: Reading) -> Session {
        if reading.changed
            && let Ok(hub) = self.hub()
        {
            hub.notify(HubNotice::Session(reading.session.session.clone()));
        }
        reading.session.session
    }
}

impl LiveState {
    /// Whether the agent can take a queued prompt as a turn of its own now, in a chat that's open.
    pub(in super::super) fn ready_for_queue(&self) -> bool {
        self.progress.queue.ready(self.progress.status, !self.is_open())
    }
}
