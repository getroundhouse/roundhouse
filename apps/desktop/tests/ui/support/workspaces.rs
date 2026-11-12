//! Workspaces as the tests find them: a window opened with a repository, the first repository, a workspace's worktree,
//! and its rows in the sidebar and in Home. `new_workspace.rs` makes them.

use std::path::PathBuf;

use gpui_kit::{ElementId, SharedString, TestAppContext};
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

impl<'a> Ui<'a> {
    /// Adds the stack's repository to the hub, then opens the window, and waits for it to list the repository.
    pub(crate) fn open_with_a_repository(cx: &'a mut TestAppContext, stack: &Stack) -> Self {
        stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
        let mut ui = Self::open(cx, stack);
        ui.wait("the repository", |ui| ui.view().repos.len() == 1);
        ui
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
