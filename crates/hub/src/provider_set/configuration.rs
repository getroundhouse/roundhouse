use crate::provider_link;
use crate::provider_link::{ADAPTER_GRACE, AdapterCommand, AdapterProvider};
use protocol::Build;
use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::model::{HubId, ProviderName};
use providers::Provider;
use serde::Deserialize;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::Arc;
use tokio::task::JoinSet;
use tracing::{error, info, warn};

pub(crate) struct ProviderSet {
    pub(super) by_name: HashMap<ProviderName, Arc<dyn Provider>>,
    /// The same as their entries in `by_name`, for what only an adapter does.
    pub(super) adapters: Vec<Arc<AdapterProvider>>,
    /// The one new workspaces go to, or why there's none.
    pub(super) default: Result<ProviderName, String>,
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
}

impl ProviderSet {
    /// Just `provider`, which new workspaces go to.
    pub(crate) fn only(provider: Arc<dyn Provider>) -> Self {
        let default = ProviderName::from(provider.name());
        Self { by_name: HashMap::from([(default.clone(), provider)]), adapters: Vec::new(), default: Ok(default) }
    }

    pub(crate) fn get(&self, name: &ProviderName) -> Option<Arc<dyn Provider>> {
        self.by_name.get(name).cloned()
    }

    /// The one new workspaces go to, and its name.
    pub(crate) fn default_provider(&self) -> Result<(&ProviderName, Arc<dyn Provider>), ApiError> {
        let unavailable = |why: String| ApiError::new(ErrorCode::Unavailable, why);
        let name = self.default.as_ref().map_err(|why| unavailable(why.clone()))?;
        let provider = self.get(name).ok_or_else(|| unavailable(format!("provider {name} isn't set up")))?;
        Ok((name, provider))
    }

    /// `local`, and each provider `file` names, whose adapters start when they're first needed. One that can't be
    /// read or doesn't parse leaves only `local`, and no provider for new workspaces, so they fail with why rather
    /// than quietly go somewhere else. Neither keeps the hub from starting.
    pub(crate) fn load(local: Arc<dyn Provider>, file: &Path, hub_id: &HubId, build: Build) -> Self {
        let mut set = Self::only(local);
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
            let open = provider_link::process(AdapterCommand { program: entry.program, args: entry.args });
            let adapter = Arc::new(AdapterProvider::new(entry.name.to_string(), hub_id.clone(), build, open));
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
