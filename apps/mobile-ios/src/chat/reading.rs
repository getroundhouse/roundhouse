//! Reading the chat shown, as client-core reads one for every client: its reply is read once it's showing in front,
//! which on a phone is while the app is in front and nothing a line opened is over the chat, and there's a hub to
//! mark it read on.

use client_core::marking::{Mark, Reading};
use gpui_kit::{Context, Window};

use super::view::ChatView;

/// What decides whether the chat shown is being read, besides what's over it.
pub(super) struct ChatReading {
    reading: Reading,
    /// The app is in front, rather than behind another or with the phone locked.
    app_in_front: bool,
    /// There's a connection to the hub to mark it read on.
    hub_here: bool,
}

impl ChatReading {
    /// A chat opened in `window`, over the connection it was opened with: in front while the window is, until the app
    /// says otherwise.
    pub(super) fn new(window: &Window) -> Self {
        Self { reading: Reading::default(), app_in_front: window.is_window_active(), hub_here: true }
    }
}

impl ChatView {
    /// Whether the app is in front.
    pub(crate) fn set_app_in_front(&mut self, in_front: bool, cx: &mut Context<Self>) {
        if self.read.app_in_front != in_front {
            self.read.app_in_front = in_front;
            self.read_showing(cx);
        }
    }

    /// Whether there's a connection to the hub.
    pub(crate) fn set_hub_here(&mut self, here: bool, cx: &mut Context<Self>) {
        if self.read.hub_here != here {
            self.read.hub_here = here;
            self.read_showing(cx);
        }
    }

    /// Marks the chat showing read, when it's being read with a reply that has just come, or that it hadn't been read
    /// with yet. For after anything that changes which chat shows, what it holds, or whether it's in front.
    pub(super) fn read_showing(&mut self, cx: &mut Context<Self>) {
        let in_front = self.read.app_in_front && !self.over.in_front();
        let showing = self.shown.as_ref().filter(|_| in_front).map(|shown| shown.session.clone());
        // Without a hub nothing is read, and a chat gone from in front meanwhile is read again once it's back.
        if !self.read.hub_here {
            self.read.reading.forget_unless(showing.as_ref().map(|session| &session.id));
            return;
        }
        if !self.read.reading.read(showing.as_ref()) {
            return;
        }
        let Some(session) = showing else { return };
        let marking = Mark::Read.chat(&self.hub, &session);
        cx.spawn(async move |this, cx| {
            if let Err(e) = marking.recv().await {
                let said = Mark::Read.couldnt_mark_the_chat(&e);
                let _ = this.update(cx, |chat, cx| chat.say(&said, cx));
            }
        })
        .detach();
    }
}
