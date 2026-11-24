use serde::{Deserialize, Serialize};

use super::super::{GithubAccountId, GithubUsername, Rev};
use super::{GithubInstallation, GithubUserId};

/// A GitHub account the hub holds a login for, as a record of its own, so every client shows the same of it. The
/// default, as [`GithubSettings::account`](crate::model::GithubSettings::account) names it, is the one the hub reads
/// GitHub as, and agents push and open pull requests as on the providers Roundhouse sets up GitHub on. The login itself
/// is in a file of the hub's, and never on a record. Disconnecting forgets the login and the record, but for an app's
/// installation, whose record stays with none, as
/// [`GithubAccountDisconnect`](crate::client::github_accounts::GithubAccountDisconnect) says.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubAccount {
    pub id: GithubAccountId,
    /// Who it is, by GitHub's number for its user, as GitHub said as the account was connected. What GitHub calls the
    /// user now is the observation `GithubUsername`.
    pub user: GithubUserNumber,
    /// `None` for an app's installation that was disconnected or uninstalled there.
    #[serde(default)]
    pub held_login: Option<HeldGithubLogin>,
    /// Where it's installed, for one that's an installation of an app of your own, whose `user` is then the app's bot.
    /// It stays once its login has gone, as when the app was removed or uninstalled there. `None` for any other.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installation: Option<GithubInstallation>,
    pub rev: Rev,
}

/// A GitHub user: its number, which never changes, and the name it goes by, which its user can change. An organization
/// is numbered and named as a user is, and so is an app's bot, such as `ada-roundhouse[bot]`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubUser {
    pub id: GithubUserId,
    pub name: GithubUsername,
}

impl GithubUser {
    /// It, by its number alone, as a record names it.
    pub fn number(&self) -> GithubUserNumber {
        GithubUserNumber { id: self.id }
    }
}

/// A GitHub user, or an organization, by its number alone, which never changes, as a record names one, since what
/// GitHub calls it is an observation. Written as `{"id":7}`, as a [`GithubUser`] is without its name, which a client
/// from before read as an object.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GithubUserNumber {
    pub id: GithubUserId,
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
    /// token it gave ends, which renewing it moves. `None` for an app's installation, whose token the hub makes again
    /// from the app's key before each ends, an hour after it was made, so its record doesn't change every hour.
    /// *Unconfirmed.*
    #[serde(default)]
    pub ends_at_ms: Option<i64>,
    /// Whether it can't be renewed, so the person has to sign in again, as when a sign-in's refresh token has ended or
    /// GitHub has refused it, or the hub couldn't keep what renewing it gave. Signing in again, or connecting a token,
    /// clears it. False from a hub from before it said. *Unconfirmed.*
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
    /// An installation of an app of your own, as [`GithubInstallation`] says, whose token, which lasts an hour, the hub
    /// makes from the app's key, and makes again before it ends, so it needs nothing of the person while the app's
    /// installed there.
    App,
    /// The login `gh` has on the hub's own machine, whose token the hub reads from `gh` each time it checks its logins,
    /// and keeps only in memory, so logging `gh` out, or switching it to another user, reaches what workers are sent
    /// within a minute. Its record says nothing of when it ends, which is `gh`'s to renew.
    Gh,
    /// One from a newer peer.
    #[serde(other)]
    Unknown,
}

crate::tokens!(
    /// The token of a GitHub login, which is all it takes to act as its account on GitHub.
    GithubToken,
);
