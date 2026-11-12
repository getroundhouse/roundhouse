//! Workspaces as the tests find them: a window opened with a repository, or opened again, the first repository, a
//! workspace's worktree, its rows in the sidebar and in Home, and its row's pencil. `new_workspace.rs` makes them.

use std::path::PathBuf;

use gpui_kit::{AppContext as _, ElementId, SharedString, TestAppContext};
use protocol::model::{RepoId, WorkspaceId};

use super::{Stack, Ui};

/// A workspace's row in the sidebar.
pub(crate) fn sidebar_row(id: &WorkspaceId) -> ElementId {
    ElementId::Name(SharedString::from(format!("workspace-{id}")))
}

/// A workspace's row in Home.
pub(crate) fn home_row(id: &WorkspaceId) -> ElementId {
    ElementId::Name(SharedString::from(format!("home-{id}")))
}

/// The pencil on a workspace's row, while something typed in it isn't sent.
pub(crate) fn pencil(id: &WorkspaceId) -> ElementId {
    ElementId::Name(SharedString::from(format!("draft-{id}")))
}

impl<'a> Ui<'a> {
    /// Adds the stack's repository to the hub, then opens the window, and waits for it to list the repository.
    pub(crate) fn open_with_a_repository(cx: &'a mut TestAppContext, stack: &Stack) -> Self {
        stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
        let mut ui = Self::open(cx, stack);
        ui.wait("the repository", |ui| ui.view().repos.len() == 1);
        ui
    }

    /// Closes the window and opens the app's again on the same hub, as quitting and opening it does, with nothing the
    /// first kept in memory. Waits for it to connect.
    pub(crate) fn open_again(&mut self, stack: &Stack) {
        self.cx.update_window(self.handle, |_, window, _| window.remove_window()).unwrap();
        let (hub, checkout) = (stack.hub.clone(), stack.checkout.clone());
        let opens = desktop::Opens::WhereItWasLeft;
        (self.handle, self.view) = self.cx.update(|cx| desktop::open_main_window(hub, checkout, opens, cx)).unwrap();
        self.seen_approval = 0;
        self.wait("the app to connect", |ui| ui.view().client().is_some());
    }

    pub(crate) fn first_repo_id(&self) -> RepoId {
        self.view().repos[0].id.clone()
    }

    pub(crate) fn worktree(&self, id: &WorkspaceId) -> PathBuf {
        let view = self.view();
        let workspace = view.workspaces.iter().find(|w| &w.id == id).unwrap();
        PathBuf::from(workspace.path.clone().unwrap())
    }
}
