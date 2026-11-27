//! Which chats have a reply that hasn't been read, and reading the one showing.

use client_core::marking::{self, Mark};
use gpui_kit::Context;
use protocol::model::{Session, SessionId};

use super::composer::Hub;
use super::{PaneEvent, Showing, WorkspacePane};

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
    /// The chat read last while on screen, as client-core reads one for every client.
    read: marking::Reading,
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
        self.transcripts_shown(cx);
        self.terminals_shown(cx);
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
            if self.drawn() {
                self.reading.read.forget_unless(in_front);
            }
            return;
        }
        // The file tab covers the chat while it shows.
        let showing = self.session().filter(|_| self.being_read() && self.showing() == Showing::Chat).cloned();
        if self.reading.read.read(showing.as_ref())
            && let Some(session) = showing
        {
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
        let marking = mark.chat(&self.linked, session);
        cx.spawn(async move |this, cx| {
            let marked = marking.recv().await;
            let _ = this.update(cx, |t, cx| {
                if let Some(chat) = t.chats.get_mut(&id)
                    && chat.marking == Some(reply)
                {
                    chat.marking = None;
                }
                if let Err(e) = marked {
                    cx.emit(PaneEvent::Error(mark.couldnt_mark_the_chat(&e)));
                }
                cx.notify();
            });
        })
        .detach();
    }
}
