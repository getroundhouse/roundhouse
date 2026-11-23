use crate::machine_environment::MachineEnvironment;
use crate::provider_link;
use crate::provider_link::{ADAPTER_GRACE, AdapterCommand, AdapterProvider};
use protocol::Build;
use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::model::{HubId, ProviderName};
use providers::Provider;
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;
use tracing::{error, info, warn};

pub(crate) struct ProviderSet {
    pub(super) by_name: HashMap<ProviderName, Arc<dyn Provider>>,
    /// The same as their entries in `by_name`, for what only an adapter does.
    pub(super) adapters: Vec<Arc<AdapterProvider>>,
    /// The one new workspaces go to, or why there's none.
    pub(super) default: Result<ProviderName, String>,
    /// The providers whose machines Roundhouse leaves GitHub to, as [`ProviderSet::leaves_github_to_machine`] says.
    pub(super) left_to_machine: BTreeSet<ProviderName>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProvidersFile {
    #[serde(default)]
    pub(super) default: Option<ProviderName>,
    #[serde(default)]
    pub(super) providers: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    pub(super) name: ProviderName,
    pub(super) program: String,
    #[serde(default)]
    pub(super) args: Vec<String>,
    /// Whether Roundhouse sets up GitHub on the provider's machines, as [`ProviderSet::leaves_github_to_machine`]
    /// says. On unless the entry says otherwise, since a machine made for a workspace rarely has a login for GitHub.
    #[serde(default = "sets_up_github")]
    pub(super) set_up_github: bool,
}

fn sets_up_github() -> bool {
    true
}

impl ProviderSet {
    /// Just `provider`, which new workspaces go to, and on whose machines Roundhouse sets up GitHub.
    pub(crate) fn only(provider: Arc<dyn Provider>) -> Self {
        let default = ProviderName::from(provider.name());
        Self {
            by_name: HashMap::from([(default.clone(), provider)]),
            adapters: Vec::new(),
            default: Ok(default),
            left_to_machine: BTreeSet::new(),
        }
    }

    /// Whether Roundhouse leaves GitHub on `name`'s machines to the machine, so its workers are given none of the
    /// default GitHub account, and git and `gh` there are as the machine has them. So it is for the local provider,
    /// whose machine is the person's own, and for a provider whose entry says `"set_up_github": false`, as one whose
    /// machines come with their own login for GitHub would. Not for any other, nor for a name the hub doesn't know.
    /// `docs/decisions/github-on-provisioned-machines.md` says why.
    pub(crate) fn leaves_github_to_machine(&self, name: &ProviderName) -> bool {
        self.left_to_machine.contains(name)
    }

    /// Every provider [`ProviderSet::leaves_github_to_machine`] is true of, in order of name.
    pub(crate) fn left_to_machine(&self) -> Vec<ProviderName> {
        self.left_to_machine.iter().cloned().collect()
    }

    /// Every other provider, whose machines Roundhouse sets up GitHub on, in order of name.
    pub(crate) fn set_up_github(&self) -> Vec<ProviderName> {
        let mut set_up: Vec<_> =
            self.by_name.keys().filter(|name| !self.leaves_github_to_machine(name)).cloned().collect();
        set_up.sort();
        set_up
    }

    pub(crate) fn get(&self, name: &ProviderName) -> Option<Arc<dyn Provider>> {
        self.by_name.get(name).cloned()
    }

    /// The one new workspaces go to, and its name.
    pub(crate) fn for_new_workspace(&self) -> Result<(&ProviderName, Arc<dyn Provider>), ApiError> {
        let unavailable = |why: String| ApiError::new(ErrorCode::Unavailable, why);
        let name = self.default.as_ref().map_err(|why| unavailable(why.clone()))?;
        let provider = self.get(name).ok_or_else(|| unavailable(format!("provider {name} isn't set up")))?;
        Ok((name, provider))
    }

    /// `local`, and each provider `file` names, whose adapters start when they're first needed and have
    /// `adapter_hello_within` to answer each hello. One that can't be read or doesn't parse leaves only `local`, and
    /// no provider for new workspaces, so they fail with why rather than quietly go somewhere else. Neither keeps the
    /// hub from starting.
    pub(crate) fn load(
        local: Arc<dyn Provider>,
        file: &Path,
        hub_id: &HubId,
        build: Build,
        adapter_hello_within: Duration,
        machine: &MachineEnvironment,
    ) -> Self {
        let local_name = ProviderName::from(local.name());
        let mut set = Self::only(local);
        set.left_to_machine.insert(local_name);
        let parsed = match std::fs::read(file) {
            Ok(bytes) => serde_json::from_slice::<ProvidersFile>(&bytes).map_err(|e| format!("it didn't parse: {e}")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return set,
            Err(e) => Err(format!("it couldn't be read: {e}")),
        };
        let parsed = match parsed {
            Ok(parsed) => parsed,
            Err(why) => {
                error!(file = %file.display(), "the providers aren't set up, so no new workspace can be made: {why}");
                set.default = Err(format!("{} names the providers, and {why}", file.display()));
                return set;
            }
        };
        for entry in parsed.providers {
            if set.by_name.contains_key(&entry.name) {
                warn!(provider = %entry.name, "leaving out a second provider of the same name");
                continue;
            }
            if !entry.set_up_github {
                set.left_to_machine.insert(entry.name.clone());
            }
            let command = AdapterCommand { program: entry.program, args: entry.args };
            let open = provider_link::process(command, machine.clone());
            let (name, hub_id) = (entry.name.to_string(), hub_id.clone());
            let adapter = Arc::new(AdapterProvider::new(name, hub_id, build, adapter_hello_within, open));
            set.by_name.insert(entry.name, adapter.clone());
            set.adapters.push(adapter);
        }
        if let Some(default) = parsed.default {
            if !set.by_name.contains_key(&default) {
                error!(provider = %default, "new workspaces go to a provider that isn't set up, so none can be made");
            }
            set.default = Ok(default);
        }
        info!(default = ?set.default, "new workspaces go to this provider");
        set
    }

    /// Says hello to each adapter, so a problem with one is logged before anything needs it.
    pub(crate) async fn say_hello(&self) {
        let mut hellos = JoinSet::new();
        for adapter in &self.adapters {
            let adapter = adapter.clone();
            hellos.spawn(async move {
                if let Err(e) = adapter.say_hello().await {
                    warn!(provider = adapter.name(), "a provider's adapter didn't say hello: {}", error_chain(&e));
                }
            });
        }
        hellos.join_all().await;
    }

    /// Ends each adapter, once it has had [`ADAPTER_GRACE`] to exit by itself.
    pub(crate) async fn shut_down(&self) {
        let mut ending = JoinSet::new();
        for adapter in &self.adapters {
            let adapter = adapter.clone();
            ending.spawn(async move { adapter.shut_down(ADAPTER_GRACE).await });
        }
        ending.join_all().await;
    }
}
