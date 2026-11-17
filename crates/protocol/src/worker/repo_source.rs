//! What a worker makes a repository's worktrees from, which every request about a workspace's worktrees names, and the
//! form a request writes it in, so a worker from before it reads what it can of it.

use serde::{Deserialize, Serialize};

/// What a worker makes a workspace's worktrees from, and names, restores, removes and sets them up from: a checkout of
/// the repository on the worker's machine, or the repository's address. A request's, so it has no `Unknown`: a worker
/// that doesn't know a kind can't read the request, and refuses it, rather than make a worktree some other way.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepoSource {
    /// The checkout at `path`, whose ignored local files and local settings a new worktree is given.
    Checkout { path: String },
    /// The repository's address, which the worker clones the first time a workspace needs it, into a clone of its own
    /// on its machine that the machine's workers share, and fetches from then on. Each worktree is made from that
    /// clone, as from a checkout. A clone has no local files or local settings to give a worktree.
    Clone { url: String },
}

impl RepoSource {
    /// The checkout's path, unless this is the repository's address.
    pub fn checkout(&self) -> Option<&str> {
        match self {
            Self::Checkout { path } => Some(path),
            Self::Clone { .. } => None,
        }
    }
}

/// How a request writes its [`RepoSource`]: as `source`, and for a checkout, beside the `repo_path` that a worker from
/// before `source` reads, so such a worker goes on making worktrees from it. A clone is written without `repo_path`,
/// so such a worker can't read the request and refuses it, rather than make a worktree of `""`. Read from `source`, or,
/// from a hub from before `source`, as after a rollback, from `repo_path` alone. `repo_path` goes with the next break of
/// the worker protocol, as "Changing the protocol" in `docs/architecture.md` says.
pub(super) mod beside_repo_path {
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::RepoSource;

    #[derive(Serialize)]
    struct Written<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        repo_path: Option<&'a str>,
        source: &'a RepoSource,
    }

    #[derive(Deserialize)]
    struct Read {
        #[serde(default)]
        repo_path: Option<String>,
        #[serde(default)]
        source: Option<RepoSource>,
    }

    pub(in super::super) fn serialize<S: Serializer>(source: &RepoSource, serializer: S) -> Result<S::Ok, S::Error> {
        Written { repo_path: source.checkout(), source }.serialize(serializer)
    }

    pub(in super::super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<RepoSource, D::Error> {
        let Read { repo_path, source } = Read::deserialize(deserializer)?;
        source
            .or_else(|| repo_path.map(|path| RepoSource::Checkout { path }))
            .ok_or_else(|| D::Error::missing_field("source"))
    }
}
