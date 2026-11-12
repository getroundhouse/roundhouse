//! A workspace that's ready, which is the only kind whose worktree can be read or worked in, since one being made has
//! no worktree yet, and an archived one's is gone. Only a ready workspace can be made into one, here, so whatever takes
//! one needn't ask again whether it's ready, or whether it has its worktree's path, which a ready one always has.

use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{Workspace, WorkspaceId, WorkspaceStatus};

use super::Hub;
use crate::error::internal;
use crate::worker_link::WorkerLink;

/// A workspace that's ready, and its worktree's path.
pub(crate) struct ReadyWorkspace {
    workspace: Workspace,
    path: String,
}

impl ReadyWorkspace {
    /// `workspace`, if it's ready. One that isn't is refused with `Conflict`.
    pub(crate) fn new(workspace: Workspace) -> Result<Self, ApiError> {
        if workspace.status != WorkspaceStatus::Ready {
            return Err(not_ready(&workspace));
        }
        let path = workspace.path.clone().ok_or_else(|| internal("a ready workspace has no path"))?;
        Ok(Self { workspace, path })
    }

    pub(crate) fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub(crate) fn id(&self) -> &WorkspaceId {
        &self.workspace.id
    }

    /// Its worktree's path on its worker's machine.
    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn into_parts(self) -> (Workspace, String) {
        (self.workspace, self.path)
    }

    pub(crate) fn into_workspace(self) -> Workspace {
        self.workspace
    }
}

/// A workspace that's ready, and its worker.
pub(crate) struct ReadyWorktree {
    ready: ReadyWorkspace,
    link: Arc<WorkerLink>,
}

impl ReadyWorktree {
    pub(crate) fn workspace(&self) -> &Workspace {
        self.ready.workspace()
    }

    pub(crate) fn path(&self) -> &str {
        self.ready.path()
    }

    pub(crate) fn link(&self) -> &Arc<WorkerLink> {
        &self.link
    }

    pub(crate) fn into_ready(self) -> ReadyWorkspace {
        self.ready
    }
}

fn not_ready(workspace: &Workspace) -> ApiError {
    ApiError::new(ErrorCode::Conflict, format!("workspace {} is {:?}", workspace.name, workspace.status))
}

impl Hub {
    /// The workspace, if it's ready.
    pub(crate) async fn ready_workspace(&self, id: &WorkspaceId) -> Result<ReadyWorkspace, ApiError> {
        ReadyWorkspace::new(self.workspace(id).await?)
    }

    /// The workspace, if it's ready, and its worker, started if it wasn't running.
    pub(crate) async fn ready_worktree(self: &Arc<Self>, id: &WorkspaceId) -> Result<ReadyWorktree, ApiError> {
        let ready = self.ready_workspace(id).await?;
        let link = self.ensure_worker(id).await?;
        Ok(ReadyWorktree { ready, link })
    }
}

#[cfg(test)]
mod tests {
    use protocol::client::ErrorCode;
    use protocol::model::{Workspace, WorkspaceStatus};

    use super::ReadyWorkspace;
    use crate::store::tests::workspace_w;

    #[test]
    fn only_a_ready_workspace_with_its_worktree_is_ready() {
        let ready = Workspace { path: Some("/w".into()), ..workspace_w(1) };
        let making = Workspace { status: WorkspaceStatus::Creating, path: None, ..workspace_w(1) };
        let archived = Workspace { status: WorkspaceStatus::Archived, ..ready.clone() };
        let pathless = Workspace { path: None, ..ready.clone() };

        let code = |workspace| ReadyWorkspace::new(workspace).err().map(|e| e.code);
        assert_eq!(ReadyWorkspace::new(ready).map(|ready| ready.path().to_owned()).ok().as_deref(), Some("/w"));
        assert_eq!(code(making), Some(ErrorCode::Conflict));
        assert_eq!(code(archived), Some(ErrorCode::Conflict));
        assert_eq!(code(pathless), Some(ErrorCode::Internal), "a ready workspace always has a path");
    }
}
