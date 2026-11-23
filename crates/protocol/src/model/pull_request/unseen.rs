//! GitHub showing the account the hub reads it as no repository where a workspace's branch's pull requests are, so
//! the hub can't say whether the branch has one.

use serde::{Deserialize, Serialize};

use super::super::{GithubLoginKind, GithubRepositoryName, GithubUsername, InstalledApp};

/// GitHub showed `username`, the default GitHub account, which the hub reads GitHub as, no repository `repository`,
/// the one a workspace's branch's pull requests are asked of, as the hub last read it: there's none, or it's private
/// and the account can't see it, as when its organization keeps out the OAuth App a sign-in is through. So the hub
/// can't say whether the branch has a pull request, and says this rather than that it has none. Sent beside the
/// workspace's pull request, which is whatever was known before, if anything was. *Unconfirmed:* the shape.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RepositoryUnseen {
    pub repository: GithubRepositoryName,
    pub username: GithubUsername,
    /// How the hub holds the account's login, which says what can be done about it.
    pub login: GithubLoginKind,
    /// Where its app is installed, for an installation of an app of your own, as [`Reach`](super::super::Reach) says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installation: Option<InstalledApp>,
    /// What GitHub said, in its own words, such as `Could not resolve to a Repository with the name 'o/r'.`
    pub said: String,
    /// GitHub's page for the OAuth App a sign-in is through, where the person signed in grants the repository's
    /// organization access to it, or asks it to, for a sign-in's account only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approve_at: Option<String>,
}

impl RepositoryUnseen {
    /// The user or organization the repository is of, who can give the account access to it.
    pub fn owner(&self) -> &str {
        self.repository.owner()
    }
}
