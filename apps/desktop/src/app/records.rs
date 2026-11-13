//! Following client-core's copy of the hub's records: what the window shows of them, what each pane is told, and
//! drawing again as time changes what's said of them.

use std::time::Duration;

use client_core::RecordsFeed;
use gpui_kit::Context;
use protocol::model::Record;

use super::AppView;
use crate::prompt_cache;
use crate::workspace_pane::WorkspacePane;

impl AppView {
    /// Passes on a record a reply carried, so what's shown needn't wait for the feed.
    pub(crate) fn take_reply(&mut self, record: Record, cx: &mut Context<Self>) {
        let Some(feed) = self.feed.clone() else { return };
        feed.take_reply(record);
        self.refresh(&feed, cx);
    }

    /// Brings what's shown up to date with client-core's copy of the records, and tells each pane what
    /// it holds for its workspace.
    pub(super) fn refresh(&mut self, feed: &RecordsFeed, cx: &mut Context<Self>) {
        let records = feed.snapshot();
        self.drop_hidden_panes(&records, cx);
        for (id, pane) in &self.panes {
            let Some(shown) = records.in_workspace(id) else { continue };
            pane.update(cx, |p, cx| p.set_records(shown, cx));
        }
        self.records = records;
        let still_there =
            self.selected.as_ref().and_then(|id| self.records.workspace(id)).is_some_and(crate::sidebar::lists);
        if !still_there {
            // The top of the sidebar, or of what it lists if every project is collapsed.
            let top = crate::sidebar::shown(self, cx).into_iter().next();
            self.selected = top.or_else(|| crate::sidebar::listed(self, cx).into_iter().next());
        }
        let has_pane = self.selected.as_ref().is_some_and(|id| self.panes.contains_key(id));
        if !still_there || !has_pane {
            self.show_selected(cx);
        }
        self.show_kept_in_dialog(cx);
        // After a reconnect, the pane on screen catches up once the records have, and the rest when they're shown.
        if let Some(pane) = self.selected_pane() {
            pane.update(cx, WorkspacePane::catch_up);
        }
        // A new connection, or a selection gone, can change which pane is on screen.
        self.read_on_screen_now(cx);
        // An agent finishing is usually what lets an update finish.
        self.finish_update_when_idle(cx);
        self.wind_cache_clock(cx);
        cx.notify();
    }

    /// Draws the window again when what the tabs, the sidebar and Home say of a chat's cache next changes, as
    /// [`prompt_cache::next_change_in_ms`] says, then winds itself for the change after. The pane on screen keeps the
    /// line above its composer up to date itself.
    fn wind_cache_clock(&mut self, cx: &mut Context<Self>) {
        let told = self.records.workspaces().iter().filter(|workspace| prompt_cache::told_of(workspace));
        let sessions = told.flat_map(|workspace| self.records.sessions(&workspace.id));
        let Some(wait_ms) = prompt_cache::next_change_in_ms(sessions, look::now_ms(cx)) else {
            self.cache_clock = None;
            return;
        };
        self.cache_clock = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(wait_ms.unsigned_abs())).await;
            let _ = this.update(cx, |t, cx| {
                t.wind_cache_clock(cx);
                cx.notify();
            });
        }));
    }
}
