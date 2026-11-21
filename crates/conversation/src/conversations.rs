//! Every chat's conversation on one hub, on this device, whichever chats an app is showing: each chat in conversation
//! mode is followed from its record, and read out while its conversation is here, so the conversation goes on while its
//! chat isn't showing, as `docs/spec/chats.md` says. An app connected to several hubs has one for each.

use std::collections::HashMap;

use client_core::Client;
use client_core::conversation::Conversation;
use protocol::model::{Session, SessionId};

use crate::Voices;

/// The conversations on one hub, on this device, on one connection to it at a time.
pub struct Conversations {
    client: Option<Client>,
    /// Each chat in conversation mode, wherever its conversation is, so its bar can say where.
    chats: HashMap<SessionId, Conversation>,
    tell: flume::Sender<()>,
    changed: flume::Receiver<()>,
}

impl Default for Conversations {
    fn default() -> Self {
        Self::new()
    }
}

impl Conversations {
    /// None yet, until there's a connection.
    pub fn new() -> Self {
        let (tell, changed) = flume::bounded(1);
        Self { client: None, chats: HashMap::new(), tell, changed }
    }

    /// Carries each conversation on, on `client`, a new connection to the hub, or the first.
    pub fn connected(&mut self, client: &Client) {
        for conversation in self.chats.values() {
            conversation.reconnect(client);
        }
        self.client = Some(client.clone());
    }

    /// Follows `sessions`, every chat's record the hub has sent: each in conversation mode is followed, and read out
    /// with `voices` while its conversation is here, and a chat whose mode has ended, or that's gone, is let go. Nothing
    /// is fetched for it: the voice reads once speech is on and ready.
    pub fn follow<'a>(&mut self, sessions: impl IntoIterator<Item = &'a Session>, voices: &Voices) {
        let Some(client) = &self.client else { return };
        let mut on = std::collections::HashSet::new();
        for session in sessions.into_iter().filter(|session| session.interaction.device().is_some()) {
            let conversation =
                self.chats.entry(session.id.clone()).or_insert_with(|| Conversation::new(session.id.clone()));
            conversation.follow(client, session, || voices.voice(), &self.tell);
            on.insert(session.id.clone());
        }
        self.chats.retain(|id, _| on.contains(id));
    }

    /// The conversation of the chat `session`, while it's in conversation mode.
    pub fn get(&self, session: &SessionId) -> Option<&Conversation> {
        self.chats.get(session)
    }

    /// Turns the mode off in the chat `session`, as typing, Type and ✕ do, telling the hub how far the voice got
    /// first.
    pub fn end(&mut self, session: &SessionId) {
        if let (Some(client), Some(conversation)) = (&self.client, self.chats.get_mut(session)) {
            conversation.end(client);
        }
    }

    /// Says that what a conversation here is reading changed. It holds one signal at a time.
    pub fn changed(&self) -> &flume::Receiver<()> {
        &self.changed
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use client_core::stand_in_hub::{self, Does};
    use client_core::{Client, Introduction};
    use fetching::serving::serve;
    use protocol::client::ClientRequest;
    use protocol::fixtures::session;
    use protocol::model::{ClientDevice, Interaction, Session};

    use client_core::conversation::Conversation;

    use super::Conversations;
    use crate::Voices;

    fn on(id: &str, device: &str) -> Session {
        let device = ClientDevice { id: device.into(), name: device.into() };
        Session { interaction: Interaction::Conversation { device: Box::new(device) }, ..session(id) }
    }

    /// Every chat whose conversation is here is read out, whatever an app shows, and nothing is fetched for the voice
    /// as one comes, since speech is turned on by itself. A chat in the mode on another device is followed for its bar. Closing a chat whose conversation is here
    /// ends the mode, and a chat whose mode has ended is let go.
    #[test]
    fn each_conversation_here_is_read_out_and_ends_as_its_chat_closes() {
        let dir = tempfile::tempdir().unwrap();
        let (tell, ended) = flume::unbounded();
        stand_in_hub::listen(&dir.path().join("hub.sock"), move |request| {
            if let ClientRequest::SessionConversationEnd(end) = request {
                let _ = tell.send(end.id);
            }
            Does::Nothing
        });
        let this = ClientDevice { id: "mac".into(), name: "Mac".into() };
        let client = Client::connect(&dir.path().join("hub.sock"), Introduction::on("desktop", this)).unwrap();
        let files = serve(Vec::new()).unwrap();
        let voices = Voices::fetched_from(dir.path().join("phonon"), "this Mac", files.base.clone());
        let mut conversations = Conversations::new();
        conversations.connected(&client);

        let (here, there, typing) = (on("here", "mac"), on("there", "studio"), session("typing"));
        conversations.follow([&here, &there, &typing], &voices);
        assert!(conversations.get(&here.id).is_some_and(Conversation::is_here));
        assert!(conversations.get(&there.id).is_some_and(|c| !c.is_here()));
        assert!(conversations.get(&typing.id).is_none());
        assert!(files.requests().is_empty(), "nothing fetched without speech on");

        let closed = Session { closed_at_ms: Some(1), ..here.clone() };
        conversations.follow([&closed, &there], &voices);
        assert_eq!(ended.recv_timeout(Duration::from_secs(10)), Ok(here.id.clone()));
        assert!(conversations.get(&here.id).is_some_and(|c| !c.is_here()), "not read again meanwhile");

        conversations.follow([&there], &voices);
        assert!(conversations.get(&here.id).is_none());
    }
}
