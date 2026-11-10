//! What the app is waiting on the hub to do to a workspace: make one, archive one or restore one.

use protocol::model::{RepoId, WorkspaceId};

/// None of these can start while another is under way, so a button or key pressed twice does it once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Busy {
    Free,
    /// Making one in this repository. It can take a while, as it fetches first.
    Creating(RepoId),
    Archiving(WorkspaceId),
    Restoring(WorkspaceId),
}

impl Busy {
    pub fn is_free(&self) -> bool {
        *self == Busy::Free
    }

    pub fn creating_in(&self) -> Option<&RepoId> {
        match self {
            Busy::Creating(repo) => Some(repo),
            Busy::Free | Busy::Archiving(_) | Busy::Restoring(_) => None,
        }
    }

    /// What the workspace says in place of its button to archive or restore it, while it waits on the hub for that,
    /// so a press shows it took.
    pub(crate) fn doing(&self, workspace: &WorkspaceId) -> Option<&'static str> {
        match self {
            Busy::Archiving(id) if id == workspace => Some("Archiving…"),
            Busy::Restoring(id) if id == workspace => Some("Restoring…"),
            Busy::Free | Busy::Creating(_) | Busy::Archiving(_) | Busy::Restoring(_) => None,
        }
    }
}
