//! Which workspace the window shows: one chosen, however it was, Back and Forward through those it has shown, and the
//! one above or below in the sidebar.

use client_core::records::Snapshot;
use gpui_kit::{App, Context};
use protocol::model::WorkspaceId;

use super::AppView;
use super::selection::Way;
use super::wrapped_step;

impl AppView {
    /// The workspace selected, which shows unless Home or Settings is in its place.
    pub fn selected(&self) -> Option<&WorkspaceId> {
        self.selection.workspace()
    }

    /// Shows the workspace, which Back then comes back from, however it was chosen.
    pub fn select(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        self.selection.choose(id);
        self.show_selection(cx);
    }

    /// Shows the workspace before or after the one selected in this window's history, as a browser's Back and Forward
    /// do, passing over those the sidebar no longer lists.
    pub(super) fn go_through_history(&mut self, way: Way, cx: &mut Context<Self>) {
        if self.back_to_what_home_covers(way) || self.selection.go(way, listed_in(&self.records)) {
            self.show_selection(cx);
        }
    }

    /// Whether [`Self::go_through_history`] has a workspace to show.
    pub(super) fn can_go_through_history(&self, way: Way) -> bool {
        self.back_to_what_home_covers(way) || self.selection.can_go(way, listed_in(&self.records))
    }

    /// Whether `way` is Back from Home, which comes back to the workspace Home covers, as a browser's Back comes back
    /// from a page to the one before it.
    fn back_to_what_home_covers(&self, way: Way) -> bool {
        way == Way::Back && self.home_showing() && self.selected().is_some()
    }

    /// Shows the workspace selected, in place of Home.
    fn show_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected().cloned() {
            self.show_in_sidebar(&id, cx);
        }
        self.close_home();
        self.show_selected(cx);
        self.say_which_pull_requests_show();
        cx.notify();
    }

    /// Moves `by` workspaces down the sidebar, in the order it shows them. From a workspace in a collapsed project,
    /// that's to the nearest one showing on that side of where its row would be.
    pub(super) fn step_selection(&mut self, by: isize, cx: &mut Context<Self>) {
        let visible = crate::sidebar::shown(self, cx);
        if visible.is_empty() {
            return;
        }
        let at = self.selected().and_then(|id| visible.iter().position(|v| v == id));
        let at = at.or_else(|| self.beside_the_selected(&visible, by, cx)).unwrap_or(0);
        let next = wrapped_step(at, by, visible.len());
        self.select(visible[next].clone(), cx);
    }

    /// Where in `visible`, the workspaces the sidebar shows, a step of `by` sets off from when the selected one is in a
    /// collapsed project: the one just above where its row would be for a step down, and the one just below for a
    /// step up, so the step lands on the other. Above the first is the last, and below the last the first.
    fn beside_the_selected(&self, visible: &[WorkspaceId], by: isize, cx: &App) -> Option<usize> {
        let selected = self.selected()?;
        let listed = crate::sidebar::listed(self, cx);
        let place = listed.iter().position(|id| id == selected)?;
        let above = listed[..place].iter().filter(|id| visible.contains(id)).count();
        Some(if by > 0 { wrapped_step(above, -1, visible.len()) } else { above % visible.len() })
    }
}

/// Whether the sidebar lists a workspace, by `records`, which Back and Forward can then show.
fn listed_in(records: &Snapshot) -> impl Fn(&WorkspaceId) -> bool + '_ {
    |id| records.workspace(id).is_some_and(crate::sidebar::lists)
}
