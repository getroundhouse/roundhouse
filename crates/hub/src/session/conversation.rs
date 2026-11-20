//! Conversation mode: which device has a chat's conversation, the one that hears and speaks, and how far it has read
//! the agent's reply out.

use protocol::client::{ApiError, ErrorCode};
use protocol::event::EventBody;
use protocol::model::{ClientDevice, CutBy, Session, SpokenThrough};

use super::{LiveItem, LiveSession, LiveState, Origin, publish};
use crate::hub::Hub;

/// Refuses what only the device that has the chat's conversation can say, from any other.
fn has_the_conversation(state: &LiveState, device: &ClientDevice) -> Result<(), ApiError> {
    if state.conversation.as_ref() == Some(device) {
        return Ok(());
    }
    Err(ApiError::new(ErrorCode::Conflict, "this device doesn't have the chat's conversation"))
}

/// Puts the conversation on `device`, or on none. A device it leaves has stopped reading out, so what it last said of
/// how far it had got goes, even if it said so after its reply was cut off.
fn put_conversation(state: &mut LiveState, device: Option<ClientDevice>) {
    if state.conversation != device && state.speaking.take().is_some() {
        publish(state, LiveItem::Rewritten);
    }
    state.conversation = device;
}

impl LiveSession {
    /// Turns conversation mode on, with the conversation on `device`, which records the rule across the chat, or moves
    /// the conversation to `device` if the mode is on already. It's decided in order with everything the chat records,
    /// so two devices turning it on at once record the rule once, and the second takes the conversation over. A reply
    /// the device it leaves was reading out is cut off where it had got to, as stopped.
    pub(crate) async fn start_conversation(&self, device: ClientDevice) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        {
            let _in_order = self.recording.lock().await;
            let on = self.state.lock().conversation.clone();
            match on {
                None => {
                    let started = EventBody::ConversationStarted { device: device.clone() };
                    let closed = |state: &LiveState| {
                        (!state.is_open()).then(|| ApiError::new(ErrorCode::Conflict, "this chat is closed"))
                    };
                    self.record_in_order(Origin::Hub, started, None, closed).await?;
                }
                Some(on) if on == device => {}
                Some(_) => {
                    self.stop_speaking(&hub).await?;
                    let (id, moving) = (self.id.clone(), device.clone());
                    hub.blocking(move |store| store.move_conversation(&id, &moving)).await?;
                }
            }
            put_conversation(&mut self.state.lock(), Some(device));
        }
        self.announce(&hub).await
    }

    /// Turns conversation mode off, from whichever device asks. A reply being read out is cut off where it had got to,
    /// as stopped.
    pub(crate) async fn end_conversation(&self) -> Result<Session, ApiError> {
        let hub = self.hub()?;
        {
            let _in_order = self.recording.lock().await;
            self.stop_speaking(&hub).await?;
            let id = self.id.clone();
            hub.blocking(move |store| store.end_conversation(&id)).await?;
            put_conversation(&mut self.state.lock(), None);
        }
        self.announce(&hub).await
    }

    /// Notes how far `device` has read the agent's reply out, or with `None` that it has read it to its end, for every
    /// subscriber's tail, as [`protocol::client::conversation::SessionSpeaking`] says.
    pub(crate) fn speaking(&self, device: &ClientDevice, through: Option<SpokenThrough>) -> Result<(), ApiError> {
        let mut state = self.state.lock();
        has_the_conversation(&state, device)?;
        if state.speaking != through {
            state.speaking = through;
            publish(&mut state, LiveItem::Rewritten);
        }
        Ok(())
    }

    /// Records that the voice reading the agent's reply out on `device` was cut off, as
    /// [`protocol::client::conversation::SessionVoiceCut`] says.
    pub(crate) async fn cut_voice(
        &self,
        device: &ClientDevice,
        through: SpokenThrough,
        by: CutBy,
    ) -> Result<(), ApiError> {
        let hub = self.hub()?;
        let _in_order = self.recording.lock().await;
        has_the_conversation(&self.state.lock(), device)?;
        self.record_cut(&hub, through, by).await
    }

    /// Cuts off the reply being read out where it had got to, as stopped, since the device reading it is about to stop.
    /// For a caller holding `recording`.
    async fn stop_speaking(&self, hub: &Hub) -> Result<(), ApiError> {
        let Some(through) = self.state.lock().speaking.clone() else { return Ok(()) };
        self.record_cut(hub, through, CutBy::Stopped).await
    }

    /// Records the cut, unless the message was cut off already, and stops the tail saying how far the reply has got.
    /// For a caller holding `recording`, so nothing else is recorded between finding no cut and recording this one.
    async fn record_cut(&self, hub: &Hub, through: SpokenThrough, by: CutBy) -> Result<(), ApiError> {
        let (id, message) = (self.id.clone(), through.message.clone());
        if !hub.blocking(move |store| store.voice_cut(&id, &message)).await? {
            self.record_in_order(Origin::Hub, EventBody::VoiceCut { through, by }, None, |_| None).await?;
        }
        let mut state = self.state.lock();
        if state.speaking.take().is_some() {
            publish(&mut state, LiveItem::Rewritten);
        }
        Ok(())
    }
}
