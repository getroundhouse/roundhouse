//! Which workspaces have a reply nobody has read, which pane is on screen to be read, and marking a workspace from the
//! sidebar.

use gpui_kit::{App, Context};
use protocol::model::{Session, WorkspaceId};

use super::AppView;
use crate::workspace_pane::{Mark, OnScreen};

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

    /// A workspace's open chats with a reply that hasn't been read, as the sidebar shows them.
    pub fn unread_chats(&self, id: &WorkspaceId, cx: &App) -> Vec<&Session> {
        let pane = self.panes.get(id).map(|pane| pane.read(cx));
        let shows_unread = |s: &&Session| pane.map_or(s.unread(), |pane| pane.shows_unread(s));
        let sessions = self.records.sessions(id).iter();
        sessions.filter(|s| s.is_open()).filter(shows_unread).collect()
    }

    /// What marking a workspace from the sidebar does now, and to which chats: reads every chat in it, or, when there's
    /// nothing to read, marks the newest reply unread again, in the chat last worked in that has one. `None` when it has
    /// no reply at all.
    pub(crate) fn workspace_marking(&self, id: &WorkspaceId, cx: &App) -> Option<(Mark, Vec<Session>)> {
        let unread: Vec<Session> = self.unread_chats(id, cx).into_iter().cloned().collect();
        if !unread.is_empty() {
            return Some((Mark::Read, unread));
        }
        self.last_replied(id).map(|session| (Mark::Unread, vec![session.clone()]))
    }

    /// The workspace's open chat last worked in that has a reply.
    fn last_replied(&self, id: &WorkspaceId) -> Option<&Session> {
        let sessions = self.records.sessions(id).iter();
        let replied = sessions.filter(|s| s.is_open() && s.has_reply());
        replied.max_by_key(|s| (s.last_active_at_ms(), s.created_at_ms))
    }

    /// Marks chats as the sidebar's menu offered when it was drawn, from the copies it had then, so a reply that has
    /// come since, while the menu was open, is left as it is rather than read unseen, or the action turned around.
    pub fn mark_chats(&mut self, mark: Mark, chats: Vec<Session>, cx: &mut Context<Self>) {
        let Some(client) = self.client() else { return };
        let marking = client.start(move |client| {
            let mark_one = |s: &Session| match mark {
                Mark::Read => client.session_mark_read(&s.id, s.reply_seq),
                Mark::Unread => client.session_mark_unread(&s.id),
            };
            chats.iter().try_for_each(|s| mark_one(s).map(drop))
        });
        cx.spawn(async move |this, cx| {
            // A hub older than this app keeps no replies, so it has none to mark.
            if let Err(e) = marking.recv().await
                && !e.is_unsupported()
            {
                let said = format!("couldn't mark the workspace {}: {e}", mark.word());
                let _ = this.update(cx, |t, cx| t.show_error(said, cx));
            }
        })
        .detach();
    }
}
