//! The repository source and worker environment settled when a workspace is created.

use protocol::model::WorkspaceId;
use protocol::provider::WorkerEnvironment;
use protocol::worker::RepoSource;
use rusqlite::OptionalExtension;

use super::super::{Store, StoreError, json_at, lock};

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
        let conn = lock(&self.reader);
        conn.query_row("SELECT repo_source, worker_environment FROM workspaces WHERE id = ?", [id.as_str()], |row| {
            let source = json_at::<RepoSource>(row, 0)?.ok_or_else(|| missing("repo_source", 0))?;
            let environment = json_at::<WorkerEnvironment>(row, 1)?.ok_or_else(|| missing("worker_environment", 1))?;
            Ok(WorkspacePlacement { source, environment })
        })
        .optional()?
        .ok_or_else(|| StoreError::NotFound { kind: "workspace", id: id.to_string() })
    }
}

fn missing(column: &str, index: usize) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Null, format!("missing {column}").into())
}
