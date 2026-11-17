//! Made-up repositories.

use crate::model::{Repo, RepoOrigin, Rev};

/// The repository `id`, named for its ID, with its checkout at `/<id>` on the hub's machine, whose address no worker
/// has said.
pub fn repo(id: &str) -> Repo {
    Repo {
        id: id.into(),
        name: id.into(),
        origin: RepoOrigin::Checkout { path: format!("/{id}") },
        created_at_ms: 0,
        rev: Rev::default(),
    }
}
