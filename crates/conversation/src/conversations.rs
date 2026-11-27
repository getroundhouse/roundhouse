//! Every chat's conversation on one hub, on this device, whichever chats an app is showing: each chat in conversation
//! mode is followed from its record, and read out while its conversation is here, so the conversation goes on while its
//! chat isn't showing, as `docs/spec/chats.md` says. An app connected to several hubs has one for each.

use std::collections::HashMap;

use client_core::conversation::Conversation;
use client_core::{Hub, Reply};
use protocol::model::{Session, SessionId};

use crate::Voices;

/// The conversations on one hub, on this device, over every connection to it.
pub struct Conversations {
    hub: Hub,
    /// Each chat in conversation mode, wherever its conversation is, so its bar can say where.
    chats: HashMap<SessionId, Conversation>,
    tell: flume::Sender<()>,
    changed: flume::Receiver<()>,
}

impl Conversations {
    /// None yet, on `hub`, which carries each on over every connection.
    pub fn new(hub: Hub) -> Self {
        let (tell, changed) = flume::bounded(1);
        Self { hub, chats: HashMap::new(), tell, changed }
    }

    /// Follows `sessions`, every chat's record the hub has sent: each in conversation mode is followed, and read out
    /// with `voices` while its conversation is here, and a chat whose mode has ended, or that's gone, is let go. Nothing
    /// is fetched for it: the voice reads once speech is on and ready.
    pub fn follow<'a>(&mut self, sessions: impl IntoIterator<Item = &'a Session>, voices: &Voices) {
        let mut on = std::collections::HashSet::new();
        for session in sessions.into_iter().filter(|session| session.interaction.device().is_some()) {
            let conversation =
                self.chats.entry(session.id.clone()).or_insert_with(|| Conversation::new(session.id.clone()));
            conversation.follow(&self.hub, session, || voices.voice(), &self.tell);
            on.insert(session.id.clone());
        }
        self.chats.retain(|id, _| on.contains(id));
    }

    /// The conversation of the chat `session`, while it's in conversation mode.
    pub fn get(&self, session: &SessionId) -> Option<&Conversation> {
        self.chats.get(session)
    }

    /// Turns the mode off in the chat `session`, as typing, Type and ✕ do, telling the hub how far the voice got
    /// first. Answers with how the hub took it, or `None` with nothing to end. When it refused, the app says why and
    /// calls [`Self::refused`].
    pub fn end(&mut self, session: &SessionId) -> Option<Reply<()>> {
        let conversation = self.chats.get_mut(session)?;
        Some(conversation.end(&self.hub))
    }

    /// The hub refused to turn the mode off in `session`, its record, so the mode is still on: the conversation
    /// carries on, read out with `voices` again while it's here, as before it was asked.
    pub fn refused(&mut self, session: &Session, voices: &Voices) {
        let Some(conversation) = self.chats.get_mut(&session.id) else { return };
        conversation.refused();
        conversation.follow(&self.hub, session, || voices.voice(), &self.tell);
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
    use client_core::{Connection, Hub, Introduction};
    use fetching::serving::serve;
    use protocol::client::{ApiError, ClientRequest, ErrorCode, ReplyOutcome};
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
        let client = Connection::connect(&dir.path().join("hub.sock"), Introduction::on("desktop", this)).unwrap();
        let files = serve(Vec::new()).unwrap();
        let voices = Voices::fetched_from(dir.path().join("phonon"), "this Mac", files.base.clone());
        let mut conversations = Conversations::new(Hub::caught_up_on(&client));

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

    /// A hub that won't turn the mode off leaves it on, so once the app hears so, the conversation carries on.
    #[test]
    fn a_conversation_the_hub_wont_end_carries_on() {
        let dir = tempfile::tempdir().unwrap();
        stand_in_hub::listen(&dir.path().join("hub.sock"), |request| {
            if let ClientRequest::SessionConversationEnd(_) = request {
                return Does::Answers(Box::new(ReplyOutcome::Error(ApiError::new(ErrorCode::Internal, "no"))));
            }
            Does::Nothing
        });
        let this = ClientDevice { id: "mac".into(), name: "Mac".into() };
        let client = Connection::connect(&dir.path().join("hub.sock"), Introduction::on("desktop", this)).unwrap();
        let files = serve(Vec::new()).unwrap();
        let voices = Voices::fetched_from(dir.path().join("phonon"), "this Mac", files.base.clone());
        let mut conversations = Conversations::new(Hub::caught_up_on(&client));
        let here = on("here", "mac");
        conversations.follow([&here], &voices);

        let ending = conversations.end(&here.id).unwrap();
        assert!(ending.wait().is_err(), "refused");
        assert!(conversations.get(&here.id).is_some_and(|c| !c.is_here()), "not read meanwhile");

        conversations.refused(&here, &voices);
        assert!(conversations.get(&here.id).is_some_and(Conversation::is_here));
    }
}
