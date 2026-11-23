use serde::{Deserialize, Serialize};

use super::{Linking, RepoOrigin};
use crate::model::{RepoId, Rev};

/// A repository added to a hub, which the interface calls a project.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(from = "RepoForm", into = "RepoForm")]
pub struct Repo {
    pub id: RepoId,
    pub name: String,
    /// Where it is, with the address it was found by: its checkout's `origin`, as a worker on the hub's machine last
    /// read it. That address is only how the hub finds the link, which says what a machine with no checkout clones.
    pub origin: RepoOrigin,
    pub link: Linking,
    pub created_at_ms: i64,
    pub rev: Rev,
}

impl Repo {
    /// The order a hub lists repositories in, and a client shows them: by name, then by ID.
    pub fn by_name(&self, other: &Repo) -> std::cmp::Ordering {
        (&self.name, &self.id).cmp(&(&other.name, &other.id))
    }
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
    /// Not known from a hub from before links.
    #[serde(default)]
    link: Linking,
    created_at_ms: i64,
    #[serde(default)]
    rev: Rev,
}

impl From<RepoForm> for Repo {
    fn from(form: RepoForm) -> Self {
        let RepoForm { id, name, path, origin, link, created_at_ms, rev } = form;
        let origin = match (origin, path) {
            (Some(origin), _) => origin,
            (None, Some(path)) => RepoOrigin::Checkout { path },
            // Neither form, which no hub writes.
            (None, None) => RepoOrigin::Unknown,
        };
        Self { id, name, origin, link, created_at_ms, rev }
    }
}

impl From<Repo> for RepoForm {
    fn from(repo: Repo) -> Self {
        let Repo { id, name, origin, link, created_at_ms, rev } = repo;
        let path = Some(origin.checkout().unwrap_or_default().to_owned());
        Self { id, name, path, origin: Some(origin), link, created_at_ms, rev }
    }
}
