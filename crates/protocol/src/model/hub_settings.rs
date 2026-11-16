//! The settings a hub carries out itself, whichever client chose them, so it keeps them and every client shows the
//! same.

use serde::{Deserialize, Serialize};

use super::{AccountId, AgentKind, GithubAccountId, Rev};

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
    /// machine, for every repository on GitHub. `None` is each machine's own login. Connecting the first GitHub account
    /// the hub has a record of makes it the default. Otherwise only
    /// [`GithubAccountSetDefault`](crate::client::github_accounts::GithubAccountSetDefault) changes it, so a default
    /// whose login is disconnected or has ended stays the default, holding no login that can be used, until it's
    /// renewed, connected again or another is chosen.
    #[serde(default)]
    pub account: Option<GithubAccountId>,
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
    pub account: AccountId,
}
