use serde::{Deserialize, Serialize};

use super::RepoUrl;

/// Where a repository is: a checkout on the hub's machine, the address it lives at, or both, and never neither, so
/// nothing can take a checkout for granted. The address is the record's, and the truth, what a worker with no checkout
/// of its own is to clone and fetch from. A linked checkout's is read from its `origin` by a worker on the hub's
/// machine, since the hub runs no git.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepoOrigin {
    /// A checkout on the hub's machine, with no address: no worker there has said one yet, or its `origin` has none.
    Checkout { path: String },
    /// An address with no checkout on the hub's machine. *Not built:* adding a repository by its address alone, which
    /// "Creating and setting up" in `docs/spec/workspaces.md` plans, so nothing makes one yet.
    Remote { url: RepoUrl },
    /// A checkout on the hub's machine, and the address its `origin` fetched from when a worker there last read it.
    Linked { path: String, url: RepoUrl },
    /// From a newer peer, or one that didn't say.
    #[default]
    #[serde(other)]
    Unknown,
}

impl RepoOrigin {
    /// The repository's checkout on the hub's machine, where it has one.
    pub fn checkout(&self) -> Option<&str> {
        match self {
            Self::Checkout { path } | Self::Linked { path, .. } => Some(path),
            Self::Remote { .. } | Self::Unknown => None,
        }
    }

    /// The address the repository lives at, where the record has one.
    pub fn url(&self) -> Option<&RepoUrl> {
        match self {
            Self::Remote { url } | Self::Linked { url, .. } => Some(url),
            Self::Checkout { .. } | Self::Unknown => None,
        }
    }
}
