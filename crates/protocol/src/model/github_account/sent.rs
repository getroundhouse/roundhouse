use serde::{Deserialize, Serialize};

use super::super::GithubAccountId;

/// What the hub sends every worker as its git credential, as it last worked it out, without the token: each machine's
/// own, the default's login, or a block, with the reason workers are given. It's the hub's to say, on its settings, as
/// [`GithubSettings::sent`](crate::model::GithubSettings::sent) says, so a client draws whether agents can push rather
/// than working it out from an account's record and its own clock, which can't see what the hub does: an
/// installation's token that has ended while the hub makes the next, a login's file only a newer build reads, or one it
/// can't read. It changes only with what's sent, so a renewal that changes only the token changes nothing of it.
/// *Unconfirmed:* on the settings, and what it says of a block.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SentCredential {
    /// Each machine's own login, as while the default is.
    MachineOwn,
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
            SentCredential::MachineOwn => default.is_none(),
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
    /// From a newer hub.
    #[serde(other)]
    Unknown,
}
