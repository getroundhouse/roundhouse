use protocol::model::Session;

use crate::{ClientError, Connection, Hub, Reply, Result, refusals};

/// What marking a chat does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Marks every reply so far read.
    Read,
    /// Marks the newest reply unread again.
    Unread,
}

impl Mark {
    /// What a chat becomes, in words.
    pub fn word(self) -> &'static str {
        match self {
            Mark::Read => "read",
            Mark::Unread => "unread",
        }
    }

    /// Marks `chat` on `hub`, from the copy of it that was shown, so a reply that has come since isn't read unseen, as
    /// [`Hub::ask`] asks.
    pub fn chat(self, hub: &Hub, chat: &Session) -> Reply<()> {
        let chat = chat.clone();
        hub.start(move |connection| self.on(connection, &chat))
    }

    /// Marks each of `chats` on `hub`, as [`Mark::chat`] does, stopping at the first the hub refuses.
    pub fn chats(self, hub: &Hub, chats: Vec<Session>) -> Reply<()> {
        hub.start(move |connection| chats.iter().try_for_each(|chat| self.on(connection, chat)))
    }

    /// Marks `chat` on `connection`.
    fn on(self, connection: &Connection, chat: &Session) -> Result<()> {
        let marked = match self {
            Mark::Read => connection.session_mark_read(&chat.id, chat.reply_seq),
            Mark::Unread => connection.session_mark_unread(&chat.id),
        };
        marked.map(drop)
    }

    /// What to say when [`Mark::chat`] failed with `e`.
    pub fn couldnt_mark_the_chat(self, e: &ClientError) -> String {
        refusals::couldnt(&format!("mark the chat {}", self.word()), e)
    }

    /// What to say when [`Mark::chats`] failed with `e`, marking a workspace's chats.
    pub fn couldnt_mark_the_workspace(self, e: &ClientError) -> String {
        refusals::couldnt(&format!("mark the workspace {}", self.word()), e)
    }
}

/// What marking a workspace does now, from its chats, and to which: reads every open chat with a reply nobody has read,
/// as `unread` says the client shows one, or, when there's none, marks the newest reply unread again, in the open chat
/// last worked in that has one. `None` when no open chat has a reply.
pub fn workspace_marking(chats: &[Session], unread: impl Fn(&Session) -> bool) -> Option<(Mark, Vec<Session>)> {
    let open = || chats.iter().filter(|chat| chat.is_open());
    let unread: Vec<Session> = open().filter(|chat| unread(chat)).cloned().collect();
    if !unread.is_empty() {
        return Some((Mark::Read, unread));
    }
    let replied = Session::last_worked_in(open().filter(|chat| chat.has_reply()));
    replied.map(|chat| (Mark::Unread, vec![chat.clone()]))
}
