//! Which chats have a reply that hasn't been read, and reading the one showing.

use gpui_kit::Context;
use protocol::model::{Seq, Session, SessionId};

use super::composer::Hub;
use super::{PaneEvent, Showing, WorkspacePane};

/// What marking a chat does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Marks every reply so far read.
    Read,
    /// Marks the newest reply unread again.
    Unread,
}

/// Whether a pane is drawn, and whether its window is in front. What's drawn in front is read while there's a hub to
/// mark it read on, which the pane knows as its [`Hub`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OnScreen {
    /// Not drawn: another workspace's pane is, or Settings or Home is, or what there is to say about the hub.
    #[default]
    No,
    /// Drawn in a window another app is in front of.
    Behind,
    /// Drawn in the window in front.
    InFront,
}

#[derive(Default)]
pub(super) struct Reading {
    on_screen: OnScreen,
    /// The chat read last while on screen, and its newest reply then. A reply that comes while it's still showing is
    /// read too, and one marked unread again stays unread until the chat is shown again.
    shown: Option<(SessionId, Seq)>,
}

impl WorkspacePane {
    /// Whether this pane is drawn, and whether its window is in front. A chat that comes to be read is, and the files
    /// are listed again if a turn ended while it wasn't.
    pub(crate) fn set_on_screen(&mut self, on_screen: OnScreen, cx: &mut Context<Self>) {
        if self.reading.on_screen != on_screen {
            self.reading.on_screen = on_screen;
            self.showing_changed(cx);
            if self.being_read() {
                self.catch_up_files(cx);
            }
        }
    }

    /// Whether this pane is drawn, whether or not it's read.
    pub(super) fn drawn(&self) -> bool {
        self.reading.on_screen != OnScreen::No
    }

    /// Whether this pane is drawn in the window in front, with a hub to mark what it shows read on.
    pub(super) fn being_read(&self) -> bool {
        self.reading.on_screen == OnScreen::InFront && self.hub == Hub::Here
    }

    /// The chat being read: the one showing, when this pane is, and the file tab doesn't cover it.
    pub(crate) fn chat_being_read(&self) -> Option<&SessionId> {
        self.active.as_ref().filter(|_| self.being_read() && self.showing() == Showing::Chat)
    }

    /// For after anything that changes which chat is showing, or what it holds, or whether it's drawn or read: what's
    /// being dictated follows its composer, the chat showing is read, and its model picker follows its list of models.
    pub(super) fn showing_changed(&mut self, cx: &mut Context<Self>) {
        self.dictation_follows_its_composer(cx);
        self.read_showing(cx);
        self.follow_models(false, cx);
    }

    /// Marks the chat showing read, when it's being read with a reply that has just come, or that the screen hasn't
    /// shown it with yet.
    fn read_showing(&mut self, cx: &mut Context<Self>) {
        // Without a hub, nothing is read. A chat marked unread while it showed is still unread once the hub is back,
        // since it's the person going elsewhere and coming back that reads it again, which they can do meanwhile: a
        // chat that isn't in front any more, because another app or chat is, is forgotten. While this pane isn't drawn,
        // that's the hub's doing, as when the app says it can't reach one, so nothing is. Another workspace, Settings
        // or Home shown meanwhile look the same as that, so coming back from those doesn't read it.
        if self.hub == Hub::Away {
            let in_front = self
                .active
                .as_ref()
                .filter(|_| self.reading.on_screen == OnScreen::InFront && self.showing() == Showing::Chat);
            if self.drawn() && self.reading.shown.as_ref().is_some_and(|(id, _)| Some(id) != in_front) {
                self.reading.shown = None;
            }
            return;
        }
        // The file tab covers the chat while it shows.
        let showing = self.session().filter(|_| self.being_read() && self.showing() == Showing::Chat).cloned();
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
