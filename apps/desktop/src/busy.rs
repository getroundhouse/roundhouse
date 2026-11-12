//! What the app is waiting on the hub for: to make a workspace, or to archive or restore one, and what a workspace's
//! record says it's on its way to, whichever app asked.

use protocol::model::{RepoId, WorkspaceStatus};

/// What this app has asked the hub to do to a workspace and had no answer to. None of these can start while another is
/// under way, so a button or key pressed twice does it once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Busy {
    Free,
    /// Making one in this repository. It can take a while, as it fetches first.
    Creating(RepoId),
    /// Archiving or restoring one. Its record says which, from the moment the hub has the request, as [`doing`] reads it.
    Moving,
}

impl Busy {
    pub fn is_free(&self) -> bool {
        *self == Busy::Free
    }

    pub fn creating_in(&self) -> Option<&RepoId> {
        match self {
            Busy::Creating(repo) => Some(repo),
            Busy::Free | Busy::Moving => None,
        }
    }
}

/// What a workspace says in place of its button to archive or restore it while its record's `status` says it's on its
/// way out or back, so every app shows it, wherever it was asked for.
pub fn doing(status: WorkspaceStatus) -> Option<&'static str> {
    match status {
        WorkspaceStatus::Archiving => Some("Archiving…"),
        WorkspaceStatus::Restoring => Some("Restoring…"),
        WorkspaceStatus::Creating | WorkspaceStatus::Ready | WorkspaceStatus::Archived | WorkspaceStatus::Unknown => {
            None
        }
    }
}
