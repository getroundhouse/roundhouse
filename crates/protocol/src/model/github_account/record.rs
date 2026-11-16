use std::fmt;

use serde::{Deserialize, Serialize};

use super::super::{GithubAccountId, GithubUsername, Rev};

/// A GitHub account the hub holds, or held, a login for, as a record of its own, so every client shows the same of it.
/// The default, as [`GithubSettings::account`](crate::model::GithubSettings::account) names it, is the one agents push
/// and open pull requests as, on every machine. The login itself is in a file of the hub's, and never on a record.
/// Disconnecting forgets the login and keeps the record, with none, since the default can still name it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubAccount {
    pub id: GithubAccountId,
    /// Who it is, as GitHub said when the hub last asked: as the account was connected, or its login renewed.
    pub user: GithubUser,
    /// `None` once its login was disconnected, or its file is gone.
    #[serde(default)]
    pub held_login: Option<HeldGithubLogin>,
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

/// What a [`GithubAccount`]'s record says of the login the hub holds for it, as
/// [`HeldLogin`](crate::model::HeldLogin) does of an agent's account. It never carries the login.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldGithubLogin {
    pub how: GithubLoginKind,
    /// When it was connected, by the hub's clock: when its token was connected, or its sign-in approved. Connecting a
    /// token to the account again, or signing in again, moves it, and the hub renewing a sign-in's token doesn't, since
    /// the person did nothing. *Unconfirmed:* the hub's renewal leaving it.
    pub connected_at_ms: i64,
    /// When it ends, by GitHub's word, converted to the hub's clock. `None` for one that doesn't end, such as a token
    /// made with no end, and for one whose end GitHub gave in a form this hub couldn't read. For a sign-in, when the
    /// token it gave ends, which renewing it moves.
    #[serde(default)]
    pub ends_at_ms: Option<i64>,
    /// Whether it can't be renewed, so the person has to sign in again, as when a sign-in's refresh token has ended or
    /// GitHub has refused it, or the hub couldn't keep what renewing it gave. Signing in again, or connecting a token,
    /// clears it. False from a hub from before it said.
    #[serde(default)]
    pub needs_sign_in: bool,
}

impl HeldGithubLogin {
    /// Whether it still works at `now_ms`, as far as the record says: it doesn't need signing in again, and it hasn't
    /// ended, or it doesn't end. GitHub can still refuse it, as when it's been revoked, which the record can't say.
    pub fn works_at(&self, now_ms: i64) -> bool {
        !self.needs_sign_in && self.ends_at_ms.is_none_or(|ends_at_ms| ends_at_ms > now_ms)
    }
}

/// How a GitHub account's login was connected, which is what kind of login the hub holds.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GithubLoginKind {
    /// A token the person made on GitHub, such as a fine-grained one, and gave a client.
    Token,
    /// Signing in with GitHub, through the hub's OAuth App, as a [`GithubSignIn`](super::GithubSignIn) did, whose token
    /// GitHub ends after a few hours, and which the hub can renew with the refresh token it was given beside it.
    SignIn,
    /// One from a newer peer.
    #[serde(other)]
    Unknown,
}

crate::tokens!(
    /// The token of a GitHub login, which is all it takes to act as its account on GitHub.
    GithubToken,
);
