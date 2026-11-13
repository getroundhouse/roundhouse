use super::{RepoId, Rev};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Repo {
    pub id: RepoId,
    pub name: String,
    pub path: String,
    pub created_at_ms: i64,
    #[serde(default)]
    pub rev: Rev,
}
