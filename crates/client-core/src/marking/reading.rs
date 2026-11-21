//! Reading the chat a client shows. A reply is read once its chat is showing in front, whether it comes while the chat
//! is showing or the chat is shown later, as `docs/spec/sidebar.md` says, and one marked unread again while it shows
//! stays unread until the chat is shown again.

use protocol::model::{Seq, Session, SessionId};

/// The chat read last while it was being read, and its newest reply then.
#[derive(Debug, Default)]
pub struct Reading {
    shown: Option<(SessionId, Seq)>,
}

impl Reading {
    /// Says which chat is being read now, if any, and whether to mark it read: it has a reply nobody has read, and
    /// either it wasn't being read before, or the reply has come since.
    pub fn read(&mut self, being_read: Option<&Session>) -> bool {
        let Some(session) = being_read else {
            self.shown = None;
            return false;
        };
        let shown_with = self.shown.as_ref().filter(|(id, _)| *id == session.id).map(|(_, reply)| *reply);
        self.shown = Some((session.id.clone(), session.reply_seq));
        session.unread() && shown_with.is_none_or(|reply| session.reply_seq > reply)
    }

    /// Without a hub nothing is read, but a chat that isn't in front any more, `in_front` being the one that is, is
    /// forgotten, since going elsewhere and coming back reads it again, once there's a hub.
    pub fn forget_unless(&mut self, in_front: Option<&SessionId>) {
        if self.shown.as_ref().is_some_and(|(id, _)| Some(id) != in_front) {
            self.shown = None;
        }
    }
}
