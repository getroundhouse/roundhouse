//! What a view reads of an agent's account, and of a GitHub account, that a worker, an agent or GitHub last said,
//! rather than what the hub decides: who an agent's account is, what's left of its usage limits, and what GitHub calls a
//! GitHub account's user. Each is read here alone, from the account's record, so that where the hub sends one apart
//! from the record, as an observation, only these change.

use std::fmt;

use protocol::model::{Account, GithubAccount, GithubUserId, GithubUsername, HeardLimits};

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
    pub fn github_username<'a>(&'a self, account: &'a GithubAccount) -> GithubName<'a> {
        GithubName::Named(&account.user.name)
    }
}

/// What GitHub calls a GitHub account's user, as a view reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GithubName<'a> {
    Named(&'a GithubUsername),
    /// Not known, until the hub has asked GitHub, by the user's number, which never changes. Drawn as the number,
    /// `#7`, wherever a name goes, and what decides from the name treats it as can't tell.
    NotKnown(GithubUserId),
}

impl<'a> GithubName<'a> {
    /// The username, where it's known.
    pub fn known(self) -> Option<&'a GithubUsername> {
        match self {
            GithubName::Named(name) => Some(name),
            GithubName::NotKnown(_) => None,
        }
    }
}

impl fmt::Display for GithubName<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GithubName::Named(name) => f.write_str(name.as_str()),
            GithubName::NotKnown(number) => write!(f, "#{number}"),
        }
    }
}
