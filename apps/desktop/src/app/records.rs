//! Following client-core's copy of the hub's records: what the window shows of them, what each pane is told, and
//! drawing again as time changes what's said of them.

use std::collections::HashMap;
use std::time::Duration;

use client_core::RecordsFeed;
use gpui_kit::Context;
use protocol::model::{Record, Session, WorkspaceId};

use super::AppView;
use crate::prompt_cache;
use crate::workspace_pane::{WorkspacePane, accounts_named};

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
        let workspaces = records.workspaces().to_vec();
        self.drop_hidden_panes(&workspaces, cx);
        for (id, pane) in &self.panes {
            let Some(workspace) = records.workspace(id).cloned() else { continue };
            let its_sessions = records.sessions(id);
            let (its_comments, its_accounts) = (records.comments(id).to_vec(), accounts_named(&records, its_sessions));
            let its_sessions = its_sessions.to_vec();
            pane.update(cx, |p, cx| p.set_records(workspace, its_sessions, its_comments, its_accounts, cx));
        }
        let mut sessions: HashMap<WorkspaceId, Vec<Session>> = HashMap::new();
        for session in records.every_session() {
            sessions.entry(session.workspace_id.clone()).or_default().push(session.clone());
        }
        self.repos = records.repos().to_vec();
        self.workspaces = workspaces;
        self.sessions = sessions;
        let still_there =
            self.selected.as_ref().is_some_and(|id| self.visible_workspaces().iter().any(|w| &w.id == id));
        if !still_there {
            // The top of the sidebar.
            self.selected = crate::sidebar::listed(self, cx).into_iter().next();
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
        let told = self.workspaces.iter().filter(|workspace| prompt_cache::told_of(workspace));
        let sessions = told.filter_map(|workspace| self.sessions.get(&workspace.id)).flatten();
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
