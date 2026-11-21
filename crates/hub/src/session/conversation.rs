//! Conversation mode: which device has a chat's conversation, the one that hears and speaks, and what the person
//! sending a prompt does to it.

use protocol::client::{ApiError, ErrorCode};
use protocol::event::EventBody;
use protocol::model::{ClientDevice, DeviceId, Interaction, Session};
use protocol::prompt::Prompt;

use super::{LiveSession, LiveState, Origin};

/// How a prompt came to be recorded, which decides what it does to conversation mode.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Sending<'a> {
    /// The person sent it from `device`, the one their client said in its hello, if it said one. Typed, it turns
    /// conversation mode off, so every prompt sent while it's on was said out loud. Said out loud, it has to come from
    /// the device that has the conversation.
    Sent { device: Option<&'a ClientDevice> },
    /// From the queue, or written by the hub, which leaves the mode as it is, since nobody sent it just now. One said
    /// out loud that waited until the mode was off goes as typed, so it gets a written reply, since nobody's listening
    /// for one.
    Queued,
}

impl Sending<'_> {
    /// The prompt as it's recorded when sent this way, while the person talks with the agent as `interaction` says,
    /// and what recording it does to that, or why it can't be recorded.
    pub(super) fn admit<F>(
        self,
        interaction: &Interaction,
        prompt: Prompt<F>,
    ) -> Result<(Prompt<F>, Option<Interaction>), ApiError> {
        match self {
            Sending::Sent { device } if prompt.spoken => {
                has_the_conversation(interaction, device).map(|()| (prompt, None))
            }
            Sending::Sent { .. } => Ok((prompt, (!interaction.is_typing()).then_some(Interaction::Typing))),
            Sending::Queued if prompt.spoken && interaction.is_typing() => {
                Ok((Prompt { spoken: false, ..prompt }, None))
            }
            Sending::Queued => Ok((prompt, None)),
        }
    }
}

/// Whether `device` has the conversation, which only it can say anything out loud in, or say how a reply was heard,
/// while conversation mode is on. Devices are the same by their IDs, so a device that's been renamed still has it.
pub(super) fn has_the_conversation(interaction: &Interaction, device: Option<&ClientDevice>) -> Result<(), ApiError> {
    let why = match (interaction.device(), device) {
        (Some(has_it), Some(asking)) if has_it.id == asking.id => return Ok(()),
        (Some(_), Some(_)) => "another device has the conversation",
        (Some(_), None) => "this client didn't say in its hello which device it's on",
        (None, _) => "conversation mode is off",
    };
    Err(ApiError::new(ErrorCode::Conflict, why))
}

/// Whether the conversation has left the device `had`, the one that had it, now that the person talks with the agent
/// as `now` says: the mode ended, or another device took it over. What `had` was reading out is settled then, since it
/// won't say.
pub(super) fn left_conversation(had: Option<&DeviceId>, now: &Interaction) -> bool {
    had.is_some_and(|had| now.device().is_none_or(|has| has.id != *had))
}

/// Puts `now` on the live session, and says whether the conversation left the device that had it, as
/// [`left_conversation`] says. A line said for the agent goes with the mode, and stays when the conversation moves.
pub(super) fn set_interaction(state: &mut LiveState, now: Interaction) -> bool {
    let left = left_conversation(state.interaction.device().map(|had| &had.id), &now);
    state.interaction = now;
    if state.interaction.is_typing() {
        super::cover_lines::nobody_listening(state);
    }
    left
}

impl LiveSession {
    /// Turns conversation mode on, with the conversation on `device`, which records the rule across the chat, or moves
    /// the conversation to `device` if the mode is on already. It's decided in order with everything the chat records,
    /// so two devices turning it on at once record the rule once, and the second takes the conversation over.
    pub(crate) async fn start_conversation(&self, device: ClientDevice) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        {
            let _in_order = self.recording.lock().await;
            let on = self.state.lock().interaction.device().map(|on| on.id == device.id);
            match on {
                None => {
                    let started = EventBody::ConversationStarted { device };
                    let closed = |state: &LiveState| {
                        if state.is_open() {
                            Ok(None)
                        } else {
                            Err(ApiError::new(ErrorCode::Conflict, "this chat is closed"))
                        }
                    };
                    self.record_in_order(Origin::Hub, started, None, closed).await?;
                }
                Some(true) => {}
                Some(false) => {
                    let (id, moving) = (self.id.clone(), device.clone());
                    hub.blocking(move |store| store.move_conversation(&id, &moving)).await?;
                    let moved = Interaction::Conversation { device: Box::new(device) };
                    if set_interaction(&mut self.state.lock(), moved) {
                        self.settle_hearing(&hub).await;
                    }
                }
            }
        }
        self.announce(&hub).await
    }

    /// Turns conversation mode off, from whichever device asks.
    pub(crate) async fn end_conversation(&self) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        {
            let _in_order = self.recording.lock().await;
            let id = self.id.clone();
            hub.blocking(move |store| store.end_conversation(&id)).await?;
            if set_interaction(&mut self.state.lock(), Interaction::Typing) {
                self.settle_hearing(&hub).await;
            }
        }
        self.announce(&hub).await
    }
}
