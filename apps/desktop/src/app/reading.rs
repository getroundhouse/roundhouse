//! Which workspaces have a reply nobody has read, which pane is on screen to be read, and marking a workspace from the
//! sidebar.

use gpui_kit::{App, Context};
use protocol::model::{Session, WorkspaceId};

use super::AppView;
use client_core::marking::{self, Mark};

use crate::workspace_pane::OnScreen;

impl AppView {
    /// Tells each pane whether it's drawn, which only the one `pane_to_draw` gives is, unless Settings or Home is, and
    /// whether the window is in front, which is kept for telling them again without asking it. For after anything
    /// that changes one of those.
    pub(super) fn read_on_screen(&mut self, in_front: bool, cx: &mut Context<Self>) {
        self.in_front = in_front;
        let hidden = !self.visible || self.settings_open() || self.home_showing();
        let drawn = self.pane_to_draw().filter(|_| !hidden).map(|pane| pane.entity_id());
        let seen = if in_front { OnScreen::InFront } else { OnScreen::Behind };
        for pane in self.panes.values() {
            let on_screen = if Some(pane.entity_id()) == drawn { seen } else { OnScreen::No };
            pane.update(cx, |p, cx| p.set_on_screen(on_screen, cx));
        }
        // Every change to the records comes through here too, so this is where a chat that needs the person is told of.
        self.tell_of_needs(cx);
    }

    /// The same, for when this isn't in the middle of updating the window, which it asks whether it's in front.
    pub(super) fn read_on_screen_now(&mut self, cx: &mut Context<Self>) {
        // Not in front when it can't tell, so nothing is read that may not have been seen.
        let in_front = self.in_front(cx).unwrap_or(false);
        self.read_on_screen(in_front, cx);
    }

    /// Whether the window is the one in front: the key window, which another app takes when it comes in front, unlike
    /// the main window. `None` when it can't tell, such as in the middle of updating the window.
    fn in_front(&self, cx: &mut App) -> Option<bool> {
        self.window.update(cx, |_, window, _| window.is_window_active()).ok()
    }

    /// Whether the sidebar shows a chat of workspace `id` unread: as its pane shows it, while it has one, since the
    /// pane knows what's been read before the hub has heard, and otherwise as the hub records it.
    pub(crate) fn shows_unread<'a>(&'a self, id: &WorkspaceId, cx: &'a App) -> impl Fn(&Session) -> bool + 'a {
        let pane = self.panes.get(id).map(|pane| pane.read(cx));
        move |chat| pane.map_or(chat.unread(), |pane| pane.shows_unread(chat))
    }

    /// A workspace's open chats with a reply that hasn't been read, as the sidebar shows them.
    #[cfg(any(test, feature = "testing"))]
    pub fn unread_chats(&self, id: &WorkspaceId, cx: &App) -> Vec<&Session> {
        let shows_unread = self.shows_unread(id, cx);
        self.records.sessions(id).iter().filter(|s| s.is_open() && shows_unread(s)).collect()
    }

    /// What marking a workspace from the sidebar does now, and to which chats, as client-core works it out from what
    /// the sidebar shows unread.
    pub(crate) fn workspace_marking(&self, id: &WorkspaceId, cx: &App) -> Option<(Mark, Vec<Session>)> {
        marking::workspace_marking(self.records.sessions(id), self.shows_unread(id, cx))
    }

    /// Marks chats as the sidebar's menu offered when it was drawn, from the copies it had then, so a reply that has
    /// come since, while the menu was open, is left as it is rather than read unseen, or the action turned around.
    pub fn mark_chats(&mut self, mark: Mark, chats: Vec<Session>, cx: &mut Context<Self>) {
        if !self.is_connected() {
            return;
        }
        let marking = mark.chats(&self.linked, chats);
        cx.spawn(async move |this, cx| {
            if let Err(e) = marking.recv().await {
                let said = mark.couldnt_mark_the_workspace(&e);
                let _ = this.update(cx, |t, cx| t.show_error(said, cx));
            }
        })
        .detach();
    }
}
