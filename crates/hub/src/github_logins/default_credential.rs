use protocol::model::{GithubUsername, InstalledApp};
use protocol::worker::{GitCredential, GithubCredential};

/// What the logins work out from the default GitHub account, under their lock: what workers give their processes for
/// github.com, as [`DefaultCredential::for_workers`] says, and, for an account's, who GitHub knows the account as, which
/// the check of reach names. Never one from a newer hub, which only a worker can be sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DefaultCredential {
    /// Each machine's own login.
    MachineOwn,
    /// An account's, which GitHub knows as `username`: the username git gives GitHub, as `credential` has it, for a
    /// token's or a sign-in's, and for an app's installation, whose git username is `x-access-token`, its bot, with
    /// where the app is installed, as `installation` says.
    Account { credential: GitCredential, username: GithubUsername, installation: Option<InstalledApp> },
    /// Nobody, saying `reason`, as [`GithubCredential::Blocked`] does.
    Blocked { reason: String },
}

impl DefaultCredential {
    /// What workers give their processes for github.com.
    pub(crate) fn for_workers(&self) -> GithubCredential {
        match self {
            DefaultCredential::MachineOwn => GithubCredential::MachineOwn,
            DefaultCredential::Account { credential, .. } => GithubCredential::Account(credential.clone()),
            DefaultCredential::Blocked { reason } => GithubCredential::Blocked { reason: reason.clone() },
        }
    }
}
