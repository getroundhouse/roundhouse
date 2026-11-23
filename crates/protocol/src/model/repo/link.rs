use serde::{Deserialize, Serialize};

use super::RepoUrl;
use crate::model::{GithubRepositoryId, GithubRepositoryName};

/// A project's link to its repository on a host: which host, and the host's ID for the repository. It's Roundhouse's
/// decision, as which pull request is a workspace's is, so it's on the record, and changes only when a person
/// changes it, by adding the checkout again. What the host says of the repository, such as what it's called now, is an
/// observation of it, asked for by the ID, so a rename changes that and leaves the link. A second host is a new
/// variant, and no change of form.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "host", rename_all = "snake_case")]
pub enum RepoLink {
    /// A repository on github.com, by GitHub's ID for it, found by an address of the form `by`, which a machine left to
    /// its own git login clones it by, since it may have a login for only one.
    Github { id: GithubRepositoryId, by: AddressForm },
    /// A repository on a host with nothing behind it to ask, by its address, which is its ID. It has nothing to follow
    /// a rename by, and nothing of pull requests.
    Plain { url: RepoUrl },
    /// From a newer peer.
    #[serde(other)]
    Unknown,
}

/// Which form of address git reaches a repository by.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AddressForm {
    /// `https://`, and `http://`.
    Https,
    /// `ssh://`, or scp's form, such as `git@github.com:ada/roundhouse.git`.
    Ssh,
    /// From a newer peer.
    #[serde(other)]
    Unknown,
}

impl AddressForm {
    /// The form `url` is in: HTTPS for `http://` and `https://`, and SSH for any other, since git reads every other
    /// address that names a host as SSH's, but for `git://`, which has no login, and which is taken as HTTPS's, the one
    /// a machine can reach a public repository by with no login.
    pub fn of(url: &RepoUrl) -> Self {
        let lower = url.as_str().to_ascii_lowercase();
        if ["https://", "http://", "git://"].iter().any(|scheme| lower.starts_with(scheme)) {
            Self::Https
        } else {
            Self::Ssh
        }
    }
}

/// Whether a project is linked to its repository on a host, as the hub has found.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Linking {
    Linked {
        link: RepoLink,
    },
    /// Its address names `repository` on github.com, and GitHub hasn't said which repository that is, as `why` says, so
    /// nothing of GitHub's works for it, cloning on a machine of its own included.
    NotLinked {
        repository: GithubRepositoryName,
        why: Unlinked,
    },
    /// It has no address to link by: its checkout's `origin` has none, or no worker on the hub's machine has read it.
    /// It works on the hub's machine, from its checkout.
    NoAddress,
    /// From a newer peer, or from a hub from before links, which says nothing of one.
    #[default]
    #[serde(other)]
    Unknown,
}

/// Why a project whose address is on github.com isn't linked, in the words the check of a push uses.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum Unlinked {
    /// The hub hasn't asked GitHub yet, as for a moment after a worker reads the address.
    NotAskedYet,
    /// There's no GitHub account connected whose login can be used, to ask GitHub as.
    NoAccount,
    /// GitHub has no such repository that any connected account can see: it isn't there, or it's private and none of
    /// theirs.
    NotFound,
    /// GitHub couldn't be asked, or didn't say, as `why` says, such as while it can't be reached.
    CouldntTell { why: String },
    /// From a newer hub.
    #[serde(other)]
    Unknown,
}

impl Linking {
    /// The link, where there is one.
    pub fn link(&self) -> Option<&RepoLink> {
        match self {
            Self::Linked { link } => Some(link),
            Self::NotLinked { .. } | Self::NoAddress | Self::Unknown => None,
        }
    }

    /// GitHub's ID for the repository, where it's linked to one on GitHub.
    pub fn github(&self) -> Option<&GithubRepositoryId> {
        match self.link()? {
            RepoLink::Github { id, .. } => Some(id),
            RepoLink::Plain { .. } | RepoLink::Unknown => None,
        }
    }
}
