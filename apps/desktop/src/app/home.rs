//! Opening Home, which shows in place of the workspace, and choosing what its list shows.

use gpui_kit::{Context, Window};
use protocol::model::RepoId;

use super::AppView;
use crate::home::{ArchiveFilter, Home};
use crate::workspace_order::Sort;

/// What shows beside the sidebar, while Settings doesn't fill the window.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Main {
    /// The selected workspace.
    #[default]
    Workspace,
    /// Home, with the selected workspace still selected under it.
    Home,
}

impl AppView {
    pub(crate) fn home_showing(&self) -> bool {
        self.main == Main::Home
    }

    /// Shows Home, with its search focused, since the composer under it is hidden. It shows what it did when it was
    /// last open.
    pub(crate) fn open_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.main = Main::Home;
        self.home.search.update(cx, |search, cx| search.focus(window, cx));
        // The chat showing is hidden, so a reply that comes now isn't read.
        self.read_on_screen(window.is_window_active(), cx);
        cx.notify();
    }

    /// Puts Home away, for the selected workspace to show, which also gives it the keys and reads it.
    pub(super) fn close_home(&mut self) {
        self.main = Main::Workspace;
    }

    /// The repository Home's list is narrowed to, while it's still there.
    pub(crate) fn home_repo(&self) -> Option<&RepoId> {
        self.home.repo.as_ref().filter(|id| self.repo(id).is_some())
    }

    pub(crate) fn filter_home(&mut self, repo: Option<RepoId>, cx: &mut Context<Self>) {
        self.change_home(cx, |home| home.repo = repo);
    }

    pub(crate) fn filter_archived_in_home(&mut self, archived: ArchiveFilter, cx: &mut Context<Self>) {
        self.change_home(cx, |home| home.archived = archived);
    }

    pub(crate) fn sort_home(&mut self, sort: Sort, cx: &mut Context<Self>) {
        self.change_home(cx, |home| home.sort = sort);
    }

    fn change_home(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Home)) {
        change(&mut self.home);
        cx.notify();
    }
}
