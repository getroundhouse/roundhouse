//! The repository source and worker environment settled when a workspace is created.

use protocol::model::WorkspaceId;
use protocol::provider::WorkerEnvironment;
use protocol::worker::RepoSource;
use rusqlite::OptionalExtension;

use super::super::{At, Column, Columns, Store, StoreError, columns, json_at, lock};

/// The repository source and worker environment settled when a workspace is created. Stored on the workspace row so
/// later operations (launches, terminals, setup, archiving, restoring, child worktrees) never query the provider's
/// adapter or flip between a checkout and a clone if the repository or adapter changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspacePlacement {
    pub source: RepoSource,
    pub environment: WorkerEnvironment,
}

impl WorkspacePlacement {
    /// A workspace made from a local checkout at `path` on the hub's machine.
    pub fn local_checkout(path: impl Into<String>) -> Self {
        Self { source: RepoSource::Checkout { path: path.into() }, environment: WorkerEnvironment::HUB_MACHINE }
    }
}

impl Store {
    /// The repository source and worker environment settled when workspace `id` was created.
    pub fn workspace_placement(&self, id: &WorkspaceId) -> Result<WorkspacePlacement, StoreError> {
        columns! {
            struct PlacementColumns { repo_source, worker_environment }
        }
        let c = PlacementColumns::at(0);
        let conn = lock(&self.reader);
        let select = format!("SELECT {} FROM workspaces WHERE id = ?", PlacementColumns::LIST);
        conn.query_row(&select, [id.as_str()], |row| {
            let source = json_at::<RepoSource>(row, c.repo_source)?.ok_or_else(|| missing(c.repo_source))?;
            let environment = json_at::<WorkerEnvironment>(row, c.worker_environment)?
                .ok_or_else(|| missing(c.worker_environment))?;
            Ok(WorkspacePlacement { source, environment })
        })
        .optional()?
        .ok_or_else(|| StoreError::NotFound { kind: "workspace", id: id.to_string() })
    }
}

fn missing(column: Column) -> rusqlite::Error {
    column.unreadable(rusqlite::types::Type::Null, format!("missing {}", column.name()).into())
}
