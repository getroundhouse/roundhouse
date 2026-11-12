//! Which chats have a reply that hasn't been read, and reading the one showing.

use gpui_kit::Context;
use protocol::model::{Seq, Session, SessionId};

use super::{PaneEvent, Showing, WorkspacePane};

/// What marking a chat does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Marks every reply so far read.
    Read,
    /// Marks the newest reply unread again.
    Unread,
}

#[derive(Default)]
pub(super) struct Reading {
    /// This is the pane showing, in the window in front, so the chat showing in it is being read.
    on_screen: bool,
    /// The chat read last while on screen, and its newest reply then. A reply that comes while it's still showing is
    /// read too, and one marked unread again stays unread until the chat is shown again.
    shown: Option<(SessionId, Seq)>,
}

impl WorkspacePane {
    /// Whether this is the pane showing, in the window in front. A chat that comes on screen is read, and the files
    /// are listed again if a turn ended while it wasn't.
    pub fn set_on_screen(&mut self, on_screen: bool, cx: &mut Context<Self>) {
        if self.reading.on_screen != on_screen {
            self.reading.on_screen = on_screen;
            self.read_showing(cx);
            if on_screen {
                self.catch_up_files(cx);
            }
        }
    }

    /// Whether this is the pane showing, in the window in front.
    pub(super) fn on_screen(&self) -> bool {
        self.reading.on_screen
    }

    /// The chat being read: the one showing, when this is the pane showing in the window in front, and the file tab
    /// doesn't cover it.
    pub(crate) fn chat_being_read(&self) -> Option<&SessionId> {
        self.active.as_ref().filter(|_| self.reading.on_screen && self.showing() == Showing::Chat)
    }

    /// Marks the chat showing read, when it's on screen with a reply that has just come, or that the screen hasn't
    /// shown it with yet. For after anything that changes which chat is showing, or what it holds.
    pub(super) fn read_showing(&mut self, cx: &mut Context<Self>) {
        // The file tab covers the chat while it shows.
        let showing = self.session().filter(|_| self.reading.on_screen && self.showing() == Showing::Chat).cloned();
        let Some(session) = showing else {
            self.reading.shown = None;
            return;
        };
        let shown_with = self.reading.shown.as_ref().filter(|(id, _)| *id == session.id).map(|(_, reply)| *reply);
        self.reading.shown = Some((session.id.clone(), session.reply_seq));
        if session.unread() && shown_with.is_none_or(|reply| session.reply_seq > reply) {
            self.held(&session.id).marking = Some(session.reply_seq);
            self.mark(&session, Mark::Read, cx);
        }
    }

    /// Whether a chat shows as having a reply that hasn't been read.
    pub fn shows_unread(&self, session: &Session) -> bool {
        session.unread()
            && self.chats.get(&session.id).and_then(|chat| chat.marking).is_none_or(|reply| reply < session.reply_seq)
    }

    /// Marks a chat, from what client-core's copy says of it.
    pub(crate) fn mark(&mut self, session: &Session, mark: Mark, cx: &mut Context<Self>) {
        let (id, reply) = (session.id.clone(), session.reply_seq);
        let target = id.clone();
        let marking = self.client.start(move |client| match mark {
            Mark::Read => client.session_mark_read(&target, reply),
            Mark::Unread => client.session_mark_unread(&target),
        });
        cx.spawn(async move |this, cx| {
            let marked = marking.recv().await;
            let _ = this.update(cx, |t, cx| {
                if let Some(chat) = t.chats.get_mut(&id)
                    && chat.marking == Some(reply)
                {
                    chat.marking = None;
                }
                match marked {
                    Ok(session) => t.took_reply(session, cx),
                    // A hub older than this app keeps no replies, so it has none to mark.
                    Err(e) if e.is_unsupported() => {}
                    Err(e) => cx.emit(PaneEvent::Error(format!("couldn't mark the chat {}: {e}", mark.word()))),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Mark {
    /// What a chat becomes, in words.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Mark::Read => "read",
            Mark::Unread => "unread",
        }
    }
}
