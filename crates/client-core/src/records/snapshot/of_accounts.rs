//! What a view reads of an agent's account, and of a GitHub account, that a worker, an agent or GitHub last said,
//! rather than what the hub decides: who an agent's account is, what's left of its usage limits, what GitHub calls a
//! GitHub account's user, and what an app's installation is on. Each is read from its observation, at the hub's level.

use std::fmt;

use protocol::model::{Account, GithubAccount, GithubUserId, GithubUsername, HeardLimits, Observation, ObservationKey};

use super::Snapshot;
use super::observed::said;

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
    /// Who `account` is, as a worker last read it from its agent's login. Each part `None` until one has.
    pub fn account_identity(&self, account: &Account) -> AccountIdentity {
        let key = ObservationKey::AccountIdentity { account: account.id.clone() };
        let held = self.observation(&key);
        let said = if let Some(Observation::AccountIdentity { identity, .. }) = held { said(identity) } else { None };
        let protocol::model::AccountIdentity { model_provider, plan, login, email } = said.cloned().unwrap_or_default();
        AccountIdentity { model_provider, plan, login, email }
    }

    /// What any agent under `account` last heard of its usage limits, with the rules every client says them by. None
    /// until one has.
    pub fn account_limits<'a>(&'a self, account: &'a Account) -> HeardLimits<'a> {
        let key = ObservationKey::AccountLimits { account: account.id.clone() };
        let held = self.observation(&key);
        let said = if let Some(Observation::AccountLimits { limits, .. }) = held { said(limits) } else { None };
        HeardLimits(said.map_or(&[], Vec::as_slice))
    }

    /// What GitHub calls `account`'s user, as the hub last asked, or the user's number while that isn't known.
    pub fn github_username<'a>(&'a self, account: &'a GithubAccount) -> GithubName<'a> {
        let key = ObservationKey::GithubUsername { account: account.id.clone() };
        let held = self.observation(&key);
        let said = if let Some(Observation::GithubUsername { username, .. }) = held { said(username) } else { None };
        said.map_or(GithubName::NotKnown(account.user.id), GithubName::Named)
    }

    /// What GitHub calls the user or organization `account`, an app's installation, is on, as the hub last asked, or
    /// its number while that isn't known. `None` for an account that isn't an installation.
    pub fn installed_on<'a>(&'a self, account: &'a GithubAccount) -> Option<GithubName<'a>> {
        let installed = account.installation.as_ref()?;
        let key = ObservationKey::InstalledOn { account: account.id.clone() };
        let held = self.observation(&key);
        let said = if let Some(Observation::InstalledOn { on, .. }) = held { said(on) } else { None };
        Some(said.map_or(GithubName::NotKnown(installed.on.id), GithubName::Named))
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
