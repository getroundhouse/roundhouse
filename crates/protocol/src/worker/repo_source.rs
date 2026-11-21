//! What a worker makes a repository's worktrees from, which every request about a workspace's worktrees names, as its
//! `source`.

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
