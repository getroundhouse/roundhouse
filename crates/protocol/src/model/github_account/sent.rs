use serde::{Deserialize, Serialize};

use super::super::GithubAccountId;

/// What the hub sends the workers of every provider it sets up GitHub on as their git credential, as it last worked it
/// out, without the token: the default's login, or a block, with the reason workers are given, as while there's no
/// default. It's the hub's to say, on its settings, as
/// [`GithubSettings::sent`](crate::model::GithubSettings::sent) says, so a client draws whether agents can push rather
/// than working it out from an account's record and its own clock, which can't see what the hub does: an
/// installation's token that has ended while the hub makes the next, a login's file only a newer build reads, or one it
/// can't read. It changes only with what's sent, so a renewal that changes only the token changes nothing of it.
/// *Unconfirmed:* on the settings, and what it says of a block.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SentCredential {
    /// Each machine's own login, which a hub from before the hub had an identity of its own sent while it had no
    /// default.
    MachineOwn,
    /// No default, so the hub reads nothing of GitHub, and the workers of the providers it sets up GitHub on are sent a
    /// block saying `reason`, which says to connect an account. *Unconfirmed.*
    NoAccount { reason: String },
    /// The login of `account`, the default, which can be used.
    Account { account: GithubAccountId },
    /// A block, since the login of `account`, the default, can't be used, for the reason `why`, saying `reason`, which
    /// says what to do, as git says it as it refuses a push.
    Blocked { account: GithubAccountId, why: UnusableLogin, reason: String },
    /// From a newer hub.
    #[serde(other)]
    Unknown,
}

impl SentCredential {
    /// Whether it was worked out for `default`, the default GitHub account, or each machine's own login with `None`.
    /// The default and what's sent for it are written one after the other, so a copy of the settings read between
    /// has a default whose credential hasn't been worked out yet, and one from a newer hub can't be told.
    pub fn is_for(&self, default: Option<&GithubAccountId>) -> bool {
        match self {
            SentCredential::MachineOwn | SentCredential::NoAccount { .. } => default.is_none(),
            SentCredential::Account { account } | SentCredential::Blocked { account, .. } => default == Some(account),
            SentCredential::Unknown => false,
        }
    }
}

/// Why the default's login can't be used, so workers are sent a block, as the hub says.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UnusableLogin {
    /// The hub holds no login for it, as once it was disconnected, or an installation was uninstalled there.
    Disconnected,
    /// A token's that has ended.
    Ended,
    /// A sign-in's token that has ended, which the hub is renewing, or an installation's, which it's making the next
    /// of, as it tries to at every check.
    Renewing,
    /// A sign-in's that can't be renewed, so the person has to sign in again.
    SignInAgain,
    /// Its file holds a login in a form only a newer build reads.
    Newer,
    /// Its record, its login's file or its profile can't be read, or the hub has no record of it.
    Unreadable,
    /// It's the `gh` login on the hub's machine, and `gh` there has no login that works: it's logged out, there's no
    /// `gh`, or GitHub refused its token.
    GhLoggedOut,
    /// It's the `gh` login on the hub's machine, and `gh` there is logged in as another GitHub user now.
    GhOtherUser,
    /// It's the `gh` login on the hub's machine, which the hub couldn't read lately, as when `gh` didn't answer, or
    /// hasn't read since it started.
    GhUnread,
    /// From a newer hub.
    #[serde(other)]
    Unknown,
}
