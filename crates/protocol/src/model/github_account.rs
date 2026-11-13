//! The GitHub accounts agents push and open pull requests as: those the hub holds a login for, and the one a machine's
//! own `gh` is logged in to.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::{GithubAccountId, GithubUsername, ProviderName, Rev};

#[cfg(test)]
mod tests;

/// A GitHub account the hub holds a login for, as a record of its own, so every client shows the same of it. The one
/// that's the default is what agents push and open pull requests as, on every machine, as
/// [`GithubSettings::account`](super::GithubSettings::account) says. The login is in a file of the hub's, and never on a
/// record. A worker is given a [`GitCredential`](crate::worker::GitCredential) made from it, which for a token is the
/// token itself, and never a sign-in's refresh token or an app's key.
///
/// Disconnecting it removes the record, announced as [`HubNotice::Removed`](crate::client::HubNotice::Removed), since
/// nothing else names an account, unlike an agent's [`Account`](super::Account), which sessions do.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubAccount {
    pub id: GithubAccountId,
    /// Who it is, as GitHub said when the hub last asked: as the account was connected, or its login renewed.
    pub user: GithubUser,
    pub held_login: HeldGithubLogin,
    #[serde(default)]
    pub rev: Rev,
}

/// A GitHub user: its number, which never changes, and the name it goes by, which its user can change.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubUser {
    pub id: GithubUserId,
    pub name: GithubUsername,
}

/// GitHub's own number for a user, which tells two users apart where their names can't, as when one renames itself
/// and another takes the name. A plain number on the wire.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct GithubUserId(u64);

impl GithubUserId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for GithubUserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// What a [`GithubAccount`]'s record says of the login the hub holds for it, as [`HeldLogin`](super::HeldLogin) does
/// of an agent's account. It never carries the login.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldGithubLogin {
    pub how: GithubLoginKind,
    /// When it was connected, by the hub's clock. Renewing it, by signing in again as the same user or connecting
    /// another token to the account, moves it.
    pub connected_at_ms: i64,
    /// When the hub can no longer use it without the person, by GitHub's word: a token's end, or for a sign-in, the
    /// end of what renews it. `None` for one that doesn't end, such as a token made without an end.
    #[serde(default)]
    pub ends_at_ms: Option<i64>,
    /// Whether a sign-in can no longer be renewed, as when what renews it has ended or been revoked, so agents can't
    /// push as it until the person signs in again. Only ever a sign-in's.
    #[serde(default)]
    pub needs_sign_in: bool,
}

/// How a GitHub account's login was connected, which is what kind of login the hub holds.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GithubLoginKind {
    /// Signing in with GitHub, which gives the hub a token that ends within hours and one that renews it.
    SignIn,
    /// A token the person made on GitHub and gave a client.
    Token,
    /// An app of the person's own, whose key the hub holds, and makes tokens with.
    App,
    #[serde(other)]
    Unknown,
}

super::tokens!(
    /// The token of a GitHub login, or of a git credential made from one, which is all it takes to act as its account.
    GithubToken,
);

/// Who a provider's machines' own `gh` is logged in to github.com as, as a record of its own, keyed by the provider, so
/// a client can say who agents there push as while the hub has no default. The local provider's is this Mac's. The hub
/// asks the first worker it connects to on each provider after it starts, as
/// [`FindGithubLogin`](crate::worker::FindGithubLogin) says, and keeps its answer, so a provider has a record once it's
/// been asked. One with none hasn't been, or its worker was too old to say.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FoundGithubLogin {
    pub provider: ProviderName,
    /// `None` when `gh` there is logged in to nothing, or isn't there.
    #[serde(default)]
    pub login: Option<GithubLogin>,
    #[serde(default)]
    pub rev: Rev,
}

/// Who a machine's own `gh` is logged in to github.com as, as its worker read it, without the token. It's `gh`'s login
/// alone: git can push with a credential helper of its own or an SSH key, which this doesn't find.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubLogin {
    pub username: GithubUsername,
}
