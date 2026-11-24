use serde::{Deserialize, Serialize};

use super::RepoOrigin;
use crate::model::{RepoId, Rev};

/// A repository added to a hub, which the interface calls a project.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Repo {
    pub id: RepoId,
    pub name: String,
    #[serde(default)]
    pub origin: RepoOrigin,
    pub created_at_ms: i64,
    #[serde(default)]
    pub rev: Rev,
}

impl Repo {
    /// The order a hub lists repositories in, and a client shows them: by name, then by ID.
    pub fn by_name(&self, other: &Repo) -> std::cmp::Ordering {
        (&self.name, &self.id).cmp(&(&other.name, &other.id))
    }
}
