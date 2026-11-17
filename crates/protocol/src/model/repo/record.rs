use serde::{Deserialize, Serialize};

use super::RepoOrigin;
use crate::model::{RepoId, Rev};

/// A repository added to a hub, which the interface calls a project.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(from = "RepoForm", into = "RepoForm")]
pub struct Repo {
    pub id: RepoId,
    pub name: String,
    pub origin: RepoOrigin,
    pub created_at_ms: i64,
    pub rev: Rev,
}

/// How a repository goes on the wire: its [`Repo::origin`] as `origin`, and its checkout as `path` beside it, for the
/// clients from before `origin`, which read only `path`.
#[derive(Serialize, Deserialize)]
struct RepoForm {
    id: RepoId,
    name: String,
    /// The checkout, written as `""` for a repository with none, since a client from before `origin` can't read a
    /// repository without it. A hub from before `origin` writes only this, so there it's always the checkout.
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    origin: Option<RepoOrigin>,
    created_at_ms: i64,
    #[serde(default)]
    rev: Rev,
}

impl From<RepoForm> for Repo {
    fn from(form: RepoForm) -> Self {
        let RepoForm { id, name, path, origin, created_at_ms, rev } = form;
        let origin = match (origin, path) {
            (Some(origin), _) => origin,
            (None, Some(path)) => RepoOrigin::Checkout { path },
            // Neither form, which no hub writes.
            (None, None) => RepoOrigin::Unknown,
        };
        Self { id, name, origin, created_at_ms, rev }
    }
}

impl From<Repo> for RepoForm {
    fn from(repo: Repo) -> Self {
        let Repo { id, name, origin, created_at_ms, rev } = repo;
        let path = Some(origin.checkout().unwrap_or_default().to_owned());
        Self { id, name, path, origin: Some(origin), created_at_ms, rev }
    }
}
