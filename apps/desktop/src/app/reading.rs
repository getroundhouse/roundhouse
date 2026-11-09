//! Which workspaces have a reply nobody has read, which pane is on screen to be read, and marking a workspace from the
//! sidebar.

use client_core::records::Record;
use gpui_kit::{App, Context};
use protocol::model::{Session, WorkspaceId};

use super::AppView;
use crate::workspace_pane::Mark;

impl AppView {
    /// Tells each pane whether it's the one being read: the selected one, with a connection, in the window in front,
    /// and not hidden by Settings or Home. For after anything that changes one of those.
    pub(super) fn read_on_screen(&mut self, in_front: bool, cx: &mut Context<Self>) {
        let hidden = self.settings_open() || self.home_showing();
        let showing = self.selected_pane().filter(|_| in_front && !hidden).map(|pane| pane.entity_id());
        for pane in self.panes.values() {
            let on_screen = Some(pane.entity_id()) == showing;
            pane.update(cx, |p, cx| p.set_on_screen(on_screen, cx));
        }
    }

    /// The same, for when this isn't in the middle of updating the window, which it asks whether it's in front.
    pub(super) fn read_on_screen_now(&mut self, cx: &mut Context<Self>) {
        // Not in front when it can't tell, so nothing is read that may not have been seen.
        let in_front = self.in_front(cx).unwrap_or(false);
        self.read_on_screen(in_front, cx);
    }

    /// A workspace's open chats with a reply that hasn't been read, as the sidebar shows them.
    pub fn unread_chats(&self, id: &WorkspaceId, cx: &App) -> Vec<&Session> {
        let pane = self.panes.get(id).map(|pane| pane.read(cx));
        let shows_unread = |s: &&Session| pane.map_or(s.unread(), |pane| pane.shows_unread(s));
        let sessions = self.sessions.get(id).into_iter().flatten();
        sessions.filter(|s| s.closed_at_ms.is_none()).filter(shows_unread).collect()
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
        let sessions = self.sessions.get(id).into_iter().flatten();
        let replied = sessions.filter(|s| s.closed_at_ms.is_none() && s.reply_seq > 0);
        replied.max_by_key(|s| (s.updated_at_ms, s.created_at_ms))
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
            chats.iter().map(mark_one).collect::<Result<Vec<Session>, _>>()
        });
        cx.spawn(async move |this, cx| {
            let marked = marking.recv().await;
            let _ = this.update(cx, |t, cx| match marked {
                Ok(sessions) => sessions.into_iter().for_each(|s| t.take_reply(Record::Session(s), cx)),
                // A hub older than this app keeps no replies, so it has none to mark.
                Err(e) if e.is_unsupported() => {}
                Err(e) => t.show_error(format!("couldn't mark the workspace {}: {e}", mark.word()), cx),
            });
        })
        .detach();
    }
}
