//! Which chats have a reply that hasn't been read, and reading the one showing.

use std::collections::HashMap;

use gpui_kit::{AppContext as _, Context};
use protocol::model::{Seq, Session, SessionId};

use super::{PaneEvent, WorkspacePane};

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
    /// Replies being marked read, by chat. They show as read meanwhile, so a reply that comes in the chat showing
    /// doesn't flash as unread first.
    marking: HashMap<SessionId, Seq>,
}

impl WorkspacePane {
    /// Whether this is the pane showing, in the window in front. A chat that comes on screen is read.
    pub fn set_on_screen(&mut self, on_screen: bool, cx: &mut Context<Self>) {
        if self.reading.on_screen != on_screen {
            self.reading.on_screen = on_screen;
            self.read_showing(cx);
        }
    }

    /// Marks the chat showing read, when it's on screen with a reply that has just come, or that the screen hasn't
    /// shown it with yet. For after anything that changes which chat is showing, or what it holds.
    pub(super) fn read_showing(&mut self, cx: &mut Context<Self>) {
        let showing = self.session().filter(|_| self.reading.on_screen).cloned();
        let Some(session) = showing else {
            self.reading.shown = None;
            return;
        };
        let shown_with = self.reading.shown.as_ref().filter(|(id, _)| *id == session.id).map(|(_, reply)| *reply);
        self.reading.shown = Some((session.id.clone(), session.reply_seq));
        if session.unread() && shown_with.is_none_or(|reply| session.reply_seq > reply) {
            self.reading.marking.insert(session.id.clone(), session.reply_seq);
            self.mark(&session, Mark::Read, cx);
        }
    }

    /// Whether a chat shows as having a reply that hasn't been read.
    pub fn shows_unread(&self, session: &Session) -> bool {
        session.unread() && self.reading.marking.get(&session.id).is_none_or(|reply| *reply < session.reply_seq)
    }

    /// Marks a chat, from what client-core's copy says of it.
    pub(crate) fn mark(&mut self, session: &Session, mark: Mark, cx: &mut Context<Self>) {
        let (client, id, reply) = (self.client.clone(), session.id.clone(), session.reply_seq);
        cx.spawn(async move |this, cx| {
            let target = id.clone();
            let marked = cx
                .background_spawn(async move {
                    match mark {
                        Mark::Read => client.session_mark_read(&target, reply),
                        Mark::Unread => client.session_mark_unread(&target),
                    }
                })
                .await;
            let _ = this.update(cx, |t, cx| {
                if t.reading.marking.get(&id) == Some(&reply) {
                    t.reading.marking.remove(&id);
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
