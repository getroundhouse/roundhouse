//! The models an agent that discovers its own offers where a chat would launch, as every client lists them.

use serde::{Deserialize, Serialize};

use super::{AccountId, AgentKind, ProviderName, RepoId, Rev};
use crate::capability::settings::ModelCatalog;

/// Where a list applies: one kind of agent, at one version of its CLI, in one project on one provider, under one
/// login, since each of those can change what it offers. A chat's list and the new-workspace dialog's are one when
/// they share all five, so the hub asks the agent once for both.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ModelListKey {
    pub agent: AgentKind,
    pub repo: RepoId,
    pub provider: ProviderName,
    /// The hub's account whose login a launch there is handed, or `None` for the login the machine has itself.
    pub account: Option<AccountId>,
    /// The version of the agent's CLI that runs there, where the provider installs it. `None` where the machine's own
    /// CLI runs, whatever its version, as on the local provider. *Not built:* a provider that installs it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_version: Option<CliVersion>,
}

/// A version of an agent's CLI, as the provider that installs it names it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct CliVersion(String);

impl std::fmt::Display for CliVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What an agent that discovers its own models offers in one place. The hub keeps it in memory rather than storing
/// it, with a revision from the counter every record's comes from, since the agent's CLI is what says it, and a list
/// from before the hub started could be out of date. It asks the agent when a client or a check first needs the list,
/// again when a client asks it to, and, where the machine's own CLI runs, once the list is old and something needs it,
/// so every client shows the same list without starting the agent's CLI each time.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ModelList {
    #[serde(flatten)]
    pub key: ModelListKey,
    /// The last list the agent gave, which is kept while it's asked again and after asking again fails. `None` until
    /// it has answered once.
    pub catalog: Option<ModelCatalog>,
    pub lookup: ModelLookup,
    pub rev: Rev,
}

/// Whether the hub is asking the agent, and how it went the last time.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ModelLookup {
    Asking,
    Answered,
    /// In the hub's words, for the person to read.
    Failed {
        message: String,
    },
    /// From a newer hub. A client shows the catalog, if there is one, and can ask again.
    #[serde(other)]
    Unknown,
}

impl std::fmt::Display for ModelListKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { agent, repo, provider, account, cli_version } = self;
        write!(f, "{} in {repo} on {provider}", agent.as_str())?;
        if let Some(account) = account {
            write!(f, " as {account}")?;
        }
        match cli_version {
            Some(version) => write!(f, " at {version}"),
            None => Ok(()),
        }
    }
}
