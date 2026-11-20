//! Conversation mode: which device has a chat's conversation, the one that hears and speaks.

use protocol::client::{ApiError, ErrorCode};
use protocol::event::EventBody;
use protocol::model::{ClientDevice, Session};

use super::{LiveSession, Origin};

impl LiveSession {
    /// Turns conversation mode on, with the conversation on `device`, which records the rule across the chat, or moves
    /// the conversation to `device` if the mode is on already. It's decided in order with everything the chat records,
    /// so two devices turning it on at once record the rule once, and the second takes the conversation over.
    pub(crate) async fn start_conversation(&self, device: ClientDevice) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        {
            let _in_order = self.recording.lock().await;
            let on = self.state.lock().conversation.clone();
            match on {
                None => {
                    let started = EventBody::ConversationStarted { device: device.clone() };
                    let closed = |state: &super::LiveState| {
                        (!state.is_open()).then(|| ApiError::new(ErrorCode::Conflict, "this chat is closed"))
                    };
                    self.record_in_order(Origin::Hub, started, None, closed).await?;
                }
                Some(on) if on == device => {}
                Some(_) => {
                    let (id, moving) = (self.id.clone(), device.clone());
                    hub.blocking(move |store| store.move_conversation(&id, &moving)).await?;
                }
            }
            self.state.lock().conversation = Some(device);
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
            self.state.lock().conversation = None;
        }
        self.announce(&hub).await
    }
}
