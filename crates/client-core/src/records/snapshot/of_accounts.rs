//! What a view reads of an agent's account, and of a GitHub account, that a worker, an agent or GitHub last said,
//! rather than what the hub decides: who an agent's account is, what's left of its usage limits, and what GitHub calls a
//! GitHub account's user. Each is read here alone, from the account's record, so that where the hub sends one apart
//! from the record, as an observation, only these change.

use protocol::model::{Account, GithubAccount, GithubUsername, HeardLimits};

use super::Snapshot;

/// Who an agent's account is, as a worker last read it from the agent's login, each part `None` where it didn't say,
/// or until one has read it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountIdentity {
    /// Who serves the model, for a person to read.
    pub model_provider: Option<String>,
    /// The subscription, for a person to read.
    pub plan: Option<String>,
    /// How it's logged in, for a person to read.
    pub login: Option<String>,
    pub email: Option<String>,
}

impl Snapshot {
    /// Who `account` is, as a worker last read it from its agent's login.
    pub fn account_identity(&self, account: &Account) -> AccountIdentity {
        AccountIdentity {
            model_provider: account.model_provider.clone(),
            plan: account.plan.clone(),
            login: account.login.clone(),
            email: account.email.clone(),
        }
    }

    /// What any agent under `account` last heard of its usage limits, with the rules every client says them by.
    pub fn account_limits<'a>(&'a self, account: &'a Account) -> HeardLimits<'a> {
        HeardLimits(&account.limits)
    }

    /// What GitHub calls `account`'s user, as the hub last asked.
    pub fn github_username<'a>(&'a self, account: &'a GithubAccount) -> &'a GithubUsername {
        &account.user.name
    }
}
