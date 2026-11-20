//! The settings a hub carries out itself, whichever client chose them, so it keeps them and every client shows the
//! same.

use serde::{Deserialize, Serialize};

use super::{AccountId, AgentKind, GithubAccountId, ProviderName, Rev, SentCredential};

/// A hub's settings. There's one record of them per hub.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HubSettings {
    /// Whether it keeps its machine from going to sleep by itself while there's work.
    pub keep_awake: bool,
    /// Whether it allows a tool's use itself when an agent with full access asks anyway, as Claude's CLI does before
    /// a command it can't check, so nobody waits on it. `None` from a hub from before it could.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_asked_in_full_access: Option<bool>,
    /// Defaults chosen on this hub, by agent kind. None means the hub predates account defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_defaults: Option<Vec<AccountDefault>>,
    /// What it says of the GitHub accounts it holds. `None` from a hub that keeps none, which is how a client tells it
    /// from one that holds none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github: Option<GithubSettings>,
    pub rev: Rev,
}

/// What the settings of a hub that keeps GitHub accounts say of them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubSettings {
    /// The default: the [`GithubAccount`](super::GithubAccount) agents push and open pull requests as, on every
    /// provider where Roundhouse sets up GitHub, for every repository on GitHub. `None` is each machine's own login. Connecting the first GitHub account
    /// the hub has a record of makes it the default. Otherwise only
    /// [`GithubAccountSetDefault`](crate::client::github_accounts::GithubAccountSetDefault) changes it, so a default
    /// whose login is disconnected or has ended stays the default, holding no login that can be used, until it's
    /// renewed, connected again or another is chosen.
    #[serde(default)]
    pub account: Option<GithubAccountId>,
    /// What the hub sends every worker for the default, as [`SentCredential`] says, so a client shows whether agents
    /// can push as the hub has it. It moves the settings' revision only when what's sent changes. `None` from a hub
    /// from before it said, whose client works nothing out in its place.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent: Option<SentCredential>,
    /// The providers whose machines Roundhouse leaves GitHub to, so their workers are given none of the default, and a
    /// workspace there says nothing of whether the default can push: the local provider, whose machine is the
    /// person's own, and any other whose entry in the hub's providers file says so. Every other provider's workers are
    /// given the default. Empty from a hub from before it said, which gave the default to every worker. *Unconfirmed.*
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub left_to_machine: Vec<ProviderName>,
}

impl GithubSettings {
    /// What the hub sends every worker for [`GithubSettings::account`], if it says, and it was worked out for that
    /// default, as [`SentCredential::is_for`] says. `None` otherwise, which says nothing of whether agents can push.
    pub fn sent_for_default(&self) -> Option<&SentCredential> {
        self.sent.as_ref().filter(|sent| sent.is_for(self.account.as_ref()))
    }

    /// Whether Roundhouse gives the workers of `provider` the default, as [`GithubSettings::left_to_machine`] says.
    pub fn sets_up(&self, provider: &ProviderName) -> bool {
        !self.left_to_machine.contains(provider)
    }
}

/// Settings to change. One left out is left as it is.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct HubSettingsChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_awake: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_asked_in_full_access: Option<bool>,
}

/// The account a chat of one agent kind on none is put on as its agent first launches. A chat already on an account, or
/// on the ambient login, stays there.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AccountDefault {
    pub agent: AgentKind,
    /// The account chosen for the kind. While `automatic`, it's the one chosen before, which choosing Automatic leaves,
    /// and which a client from before Automatic shows as the default.
    pub account: AccountId,
    /// Whether the kind's chats are put on whichever login added for it has the most of its week to spare, as
    /// [`AccountSetAutomatic`](crate::client::accounts::AccountSetAutomatic) says, rather than on `account`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub automatic: bool,
}
