//! Conversation mode in one chat on this device: reading its replies out while the conversation is here, from when the
//! chat's record says it came here until it says it went, and what the bar shows meanwhile.

use protocol::model::{DeviceId, Session, SessionId};
use protocol::tail::Tail;

use super::bar::{Bar, HereNow, Shows};
use super::reading_out::{ReadingOut, Voice};
use crate::Hub;
use crate::client::Reply;
use protocol::client::conversation::SessionConversationEnd;

/// One chat's conversation on this device, while the chat is in conversation mode.
pub struct Conversation {
    /// The device this client is on, as the hub takes it, once there's been a connection.
    this: Option<DeviceId>,
    session: SessionId,
    reading: Option<ReadingOut>,
    /// This device turned the mode off, and the chat's record doesn't say so yet, so it doesn't read again meanwhile.
    left: bool,
    bar: Bar,
}

impl Conversation {
    pub fn new(session: SessionId) -> Self {
        Self { this: None, session, reading: None, left: false, bar: Bar::default() }
    }

    /// Follows the chat's record, `session`, on `hub`: once the conversation is on the device this client is on,
    /// starts reading its replies out with the voice `voice` makes, saying on `tell` when what it reads changes, and
    /// once it isn't, stops. Once the chat is closed here, turns the mode off, as closing its tab stops the
    /// conversation.
    pub fn follow<V: Voice>(
        &mut self,
        hub: &Hub,
        session: &Session,
        voice: impl FnOnce() -> V,
        tell: &flume::Sender<()>,
    ) {
        debug_assert_eq!(session.id, self.session);
        self.this = hub.device();
        self.bar.saw(session, self.this.as_ref());
        let here = session.interaction.device().is_some_and(|device| Some(&device.id) == self.this.as_ref());
        self.left &= here;
        if here && !session.is_open() && !self.left {
            // The chat is closed here, so nothing shows the hub refusing.
            drop(self.end(hub));
            return;
        }
        let reads = here && !self.left;
        if reads != self.reading.is_some() {
            self.reading = reads.then(|| ReadingOut::start(hub, session, voice(), tell.clone()));
        }
    }

    /// Turns the mode off, as typing, Type and ✕ do, on `hub`: how far the voice got, and how each reply ended,
    /// reach the hub first, so it needn't settle them. Until the chat's record says the mode is off, the conversation
    /// isn't read here again. Asked again meanwhile, it asks the hub again. Answers with how the hub took it: a
    /// client says why when it refused, and has the conversation carry on with [`Self::refused`].
    pub fn end(&mut self, hub: &Hub) -> Reply<()> {
        self.left = true;
        if let Some(reading) = self.reading.take() {
            return reading.leave();
        }
        let id = self.session.clone();
        // Nothing is being read, so there's nothing to say first.
        hub.start(move |client| client.call(SessionConversationEnd { id }).map(drop))
    }

    /// The hub refused to turn the mode off, so it's still on: the conversation carries on, read here again from the
    /// next [`Self::follow`] while it's here, as before it was asked.
    pub fn refused(&mut self) {
        self.left = false;
    }

    /// Whether the conversation is here, so this device hears and reads.
    pub fn is_here(&self) -> bool {
        self.reading.is_some()
    }

    /// The person started talking: what's being read is talked over, and nothing is read over them.
    pub fn talk(&self) {
        if let Some(reading) = &self.reading {
            reading.talk();
        }
    }

    /// The person stopped talking, having `sent` what they said, or dropped it.
    pub fn let_go(&self, sent: bool) {
        if let Some(reading) = &self.reading {
            reading.let_go(sent);
        }
    }

    /// The person stopped the voice, or the agent.
    pub fn stop_voice(&self) {
        if let Some(reading) = &self.reading {
            reading.stop();
        }
    }

    /// Whether the voice has something it hasn't finished reading.
    pub fn reading(&self) -> bool {
        self.reading.as_ref().is_some_and(ReadingOut::reading)
    }

    /// What the bar shows, as the record `session` and its tail `tail` say, and this device's `here`.
    pub fn shows(&self, session: &Session, tail: &Tail, here: &HereNow) -> Shows {
        debug_assert_eq!(session.id, self.session);
        self.bar.shows(session, tail, self.this.as_ref(), here)
    }

    /// Whether the voice is reading a reply, rather than nothing or only a line said for the agent, which the bar
    /// shows as the agent at work.
    pub fn reading_a_reply(&self) -> bool {
        self.reading.as_ref().is_some_and(ReadingOut::reading_a_reply)
    }
}
