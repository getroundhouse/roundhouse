use protocol::model::{GithubAccountId, GithubUsername, InstalledApp, SentCredential, UnusableLogin};
use protocol::worker::{GitCredential, GithubCredential};

/// What the logins work out from the default GitHub account, under their lock, in one place: what workers give their
/// processes for github.com, as [`DefaultCredential::for_workers`] says, for an account's, who GitHub knows the account
/// as, which the check of reach names, and what the hub's settings say of it, without the token, as
/// [`DefaultCredential::recorded`] says, so none of them can disagree. Never one from a newer hub, which only a worker
/// can be sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DefaultCredential {
    /// No default, so the hub reads nothing of GitHub, and workers are sent a block saying `reason`, which says to
    /// connect an account.
    NoAccount { reason: String },
    /// The login of `account`, the default, which GitHub knows as `username`: the username git gives GitHub, as
    /// `credential` has it, for a token's or a sign-in's, and for an app's installation, whose git username is
    /// `x-access-token`, its bot, with where the app is installed, as `installation` says.
    Account {
        account: GithubAccountId,
        credential: GitCredential,
        username: GithubUsername,
        installation: Option<InstalledApp>,
    },
    /// Nobody, saying `reason`, as [`GithubCredential::Blocked`] does, since the login of `account`, the default, can't
    /// be used, as `why` says. `account` is `None` when the hub couldn't read which is the default.
    Blocked { reason: String, account: Option<GithubAccountId>, why: UnusableLogin },
}

impl DefaultCredential {
    /// What workers give their processes for github.com.
    pub(crate) fn for_workers(&self) -> GithubCredential {
        match self {
            DefaultCredential::Account { credential, .. } => GithubCredential::Account(credential.clone()),
            DefaultCredential::NoAccount { reason } | DefaultCredential::Blocked { reason, .. } => {
                GithubCredential::Blocked { reason: reason.clone() }
            }
        }
    }

    /// What the hub's settings say workers are sent, without the token, so a renewal that changes only the token
    /// changes nothing of it. `None` when the hub couldn't read which is the default, since the default and this are
    /// on the same settings, which it couldn't read either.
    pub(crate) fn recorded(&self) -> Option<SentCredential> {
        Some(match self {
            DefaultCredential::NoAccount { reason } => SentCredential::NoAccount { reason: reason.clone() },
            DefaultCredential::Account { account, .. } => SentCredential::Account { account: account.clone() },
            DefaultCredential::Blocked { reason, account, why } => {
                SentCredential::Blocked { account: account.clone()?, why: *why, reason: reason.clone() }
            }
        })
    }
}
