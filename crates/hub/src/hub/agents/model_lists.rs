//! The model lists the hub holds, in memory, and the lookups that fill them, one at a time for each.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::capability::settings::ModelCatalog;
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{ModelList, ModelListKey, ModelLookup, RecordKey, RepoOrigin};
use tokio::sync::{Mutex as AsyncMutex, watch};
use tokio::time::Instant;
use tracing::warn;

use super::super::Hub;
use crate::jobs::{Ended, JobKind, Jobs, Limits, NotStarted, step, uncut};
use crate::store::Store;

/// How old a list from the machine's own CLI gets before the next need of it asks the agent again, in the background,
/// since whoever looks after that machine can update the CLI at any time, and nothing tells the hub. *Unconfirmed.*
const ASK_AGAIN_AFTER: Duration = Duration::from_hours(1);

/// A lookup's limits. Its work is past the preflight's own limit and the grace its process has to end, so the preflight
/// ends its process itself, and this bounds what comes before it, which is the provider's answer to `OpenPreflight`.
/// Its after-step only records that it failed.
const LOOKUP: Limits = Limits { work: Duration::from_secs(60), after: Duration::from_secs(5) };

/// How long a check waits for a lookup: as long as the lookup can take, and a moment beyond, as the hub stops.
const WAIT_FOR_LOOKUP: Duration = LOOKUP.work.saturating_add(LOOKUP.after).saturating_add(Duration::from_secs(1));

pub(crate) struct ModelLists {
    /// Held while a list changes and takes its revision, and while every record is listed, so a list of records has
    /// every model list whose revision it takes in.
    changing: AsyncMutex<()>,
    held: Mutex<HashMap<ModelListKey, Held>>,
    lookups: Jobs<ModelListKey>,
}

struct Held {
    list: watch::Sender<ModelList>,
    /// When the agent last answered, for asking again once the list is old.
    answered_at: Option<Instant>,
}

/// Where a list's lookup asks: the project's checkout, on the list's provider.
#[derive(Clone)]
pub(super) struct Lookup {
    pub(super) key: ModelListKey,
    pub(super) origin: RepoOrigin,
}

impl Default for ModelLists {
    fn default() -> Self {
        Self { changing: AsyncMutex::new(()), held: Mutex::new(HashMap::new()), lookups: Jobs::new() }
    }
}

impl ModelLists {
    /// Lists the records along with `list`, under the lock a list's change takes, so the list's revision takes in
    /// none it lacks.
    pub(crate) async fn listed_with<T>(&self, list: impl Future<Output = T>) -> (T, Vec<ModelList>) {
        let _changing = self.changing.lock().await;
        let listed = list.await;
        (listed, self.held.lock().values().map(|held| held.list.borrow().clone()).collect())
    }

    /// Stops every lookup where it is, as the hub stops.
    pub(crate) fn stop(&self) {
        self.lookups.stop();
    }

    /// Whether the agent is due to be asked for the list at `key`, as [`Hub::ensure_model_list`] says.
    fn due(&self, key: &ModelListKey, again: bool) -> Due {
        let held = self.held.lock();
        let Some(held) = held.get(key) else { return Due::Yes { catalog: None } };
        let list = held.list.borrow().clone();
        let old = key.cli_version.is_none() && held.answered_at.is_none_or(|at| at.elapsed() >= ASK_AGAIN_AFTER);
        let due = match &list.lookup {
            ModelLookup::Asking => false,
            _ if again => true,
            ModelLookup::Failed { .. } => false,
            ModelLookup::Answered | ModelLookup::Unknown => old,
        };
        if due { Due::Yes { catalog: list.catalog } } else { Due::No(Box::new(list)) }
    }

    fn set(&self, list: ModelList, answered: bool) {
        let mut held = self.held.lock();
        let now = answered.then(Instant::now);
        if let Some(held) = held.get_mut(&list.key) {
            held.answered_at = now.or(held.answered_at);
            held.list.send_replace(list);
        } else {
            held.insert(list.key.clone(), Held { list: watch::Sender::new(list), answered_at: now });
        }
    }
}

/// Whether to ask the agent for a list.
#[derive(Debug, PartialEq)]
enum Due {
    /// Not now, so the list as it is stands.
    No(Box<ModelList>),
    /// Yes, keeping the last catalog it gave meanwhile, if it gave one.
    Yes { catalog: Option<ModelCatalog> },
}

impl Hub {
    /// The list `lookup` is for, as it is now, having started asking the agent if the hub has no list there, or if
    /// `again`, or if the machine's own CLI gave it long enough ago, unless it's asking already. A list that failed is
    /// asked again only when asked to, so a client's Retry does it rather than every read.
    pub(super) async fn ensure_model_list(
        self: &Arc<Self>,
        lookup: Lookup,
        again: bool,
    ) -> Result<ModelList, ApiError> {
        let lists = &self.model_lists;
        let _changing = lists.changing.lock().await;
        let catalog = match lists.due(&lookup.key, again) {
            Due::No(list) => return Ok(*list),
            Due::Yes { catalog } => catalog,
        };
        let rev = self.blocking(Store::next_rev).await?;
        let key = lookup.key.clone();
        let asking = ModelList { key: key.clone(), catalog: catalog.clone(), lookup: ModelLookup::Asking, rev };
        let list = match self.start_lookup(lookup) {
            Ok(()) => asking,
            Err(not) => {
                let message = match not {
                    NotStarted::KeyBusy | NotStarted::KindBusy => {
                        "the hub is asking agents about as many places as it can at once, so try again in a moment"
                    }
                    NotStarted::Stopped => "the hub is stopping",
                };
                ModelList { key, catalog, lookup: ModelLookup::Failed { message: message.into() }, rev }
            }
        };
        lists.set(list.clone(), false);
        self.notify(HubNotice::ModelList(list.clone()));
        Ok(list)
    }

    /// The list `lookup` is for, once the agent has said what it offers there, or has failed to, starting it as
    /// [`Hub::ensure_model_list`] does. The catalog is the last the agent gave, which a failure since keeps. One that
    /// failed with none is asked for again, since what checks it is something asked for, such as a chat, rather than a
    /// read, and a client with no Retry, such as the command line, would otherwise be refused until another asked.
    pub(super) async fn model_catalog(self: &Arc<Self>, lookup: Lookup) -> Result<ModelCatalog, ApiError> {
        let key = lookup.key.clone();
        let failed_with_none = self.model_lists.held.lock().get(&key).is_some_and(|held| {
            let list = held.list.borrow();
            matches!(list.lookup, ModelLookup::Failed { .. }) && list.catalog.is_none()
        });
        self.ensure_model_list(lookup, failed_with_none).await?;
        let Some(mut list) = self.model_lists.held.lock().get(&key).map(|held| held.list.subscribe()) else {
            return Err(ApiError::new(ErrorCode::Unavailable, "the hub let go of the list of models meanwhile"));
        };
        let answered = tokio::time::timeout(WAIT_FOR_LOOKUP, list.wait_for(|list| list.lookup != ModelLookup::Asking));
        let unavailable = |why: String| ApiError::new(ErrorCode::Unavailable, why);
        let list = match answered.await {
            Ok(Ok(list)) => list.clone(),
            Ok(Err(_)) => return Err(unavailable("the hub let go of the list of models meanwhile".into())),
            Err(_) => return Err(unavailable("the agent didn't say which models it offers in time".into())),
        };
        match (list.catalog, list.lookup) {
            (Some(catalog), _) => Ok(catalog),
            (None, ModelLookup::Failed { message }) => Err(unavailable(message)),
            (None, ModelLookup::Asking | ModelLookup::Answered | ModelLookup::Unknown) => {
                Err(unavailable("the agent didn't say which models it offers".into()))
            }
        }
    }

    /// Lets go of every list whose key is `gone`, as when which login a launch is handed changes, so a client asks
    /// again for the list where it is, under the login it would be handed now. A lookup still running is called off,
    /// and finds its list gone.
    pub(crate) async fn forget_model_lists(&self, gone: impl Fn(&ModelListKey) -> bool) {
        let lists = &self.model_lists;
        let _changing = lists.changing.lock().await;
        let gone: Vec<ModelListKey> = {
            let mut held = lists.held.lock();
            let gone: Vec<_> = held.keys().filter(|key| gone(key)).cloned().collect();
            for key in &gone {
                held.remove(key);
                lists.lookups.call_off(key);
            }
            gone
        };
        for key in gone {
            match self.blocking(Store::next_rev).await {
                Ok(rev) => self.notify(HubNotice::Removed { record: RecordKey::ModelList(key), rev }),
                Err(e) => warn!(list = %key, "letting go of a list of models: {}", e.message),
            }
        }
    }

    fn start_lookup(self: &Arc<Self>, lookup: Lookup) -> Result<(), NotStarted> {
        let key = lookup.key.clone();
        let hub = Arc::clone(self);
        let work = async move {
            step("asking the agent which models it offers");
            let answer = hub.look_up_models(&lookup).await;
            uncut(async move { hub.settle_model_list(&lookup.key, answer).await }).await;
        };
        let (hub, ending) = (Arc::clone(self), key.clone());
        let after = move |ended: Ended| async move {
            if !ended.finished_or_ran_uncut() {
                let failed =
                    ApiError::new(ErrorCode::Unavailable, "the agent didn't say which models it offers in time");
                hub.settle_model_list(&ending, Err(failed)).await;
            }
        };
        self.model_lists.lookups.start(JobKind::LookUpModels, key, LOOKUP, work, after)
    }

    /// Records what the agent answered, or that it failed, keeping the last catalog it gave, unless the hub has let go
    /// of the list meanwhile.
    async fn settle_model_list(&self, key: &ModelListKey, answer: Result<ModelCatalog, ApiError>) {
        let lists = &self.model_lists;
        let _changing = lists.changing.lock().await;
        let Some(was) = lists.held.lock().get(key).map(|held| held.list.borrow().clone()) else { return };
        // Without a revision of its own, clients go on showing the copy they have, but a check waiting still hears.
        let rev = match self.blocking(Store::next_rev).await {
            Ok(rev) => rev,
            Err(e) => {
                warn!(list = %key, "recording which models an agent offers: {}", e.message);
                was.rev
            }
        };
        let answered = answer.is_ok();
        let list = match answer {
            Ok(catalog) => ModelList { catalog: Some(catalog), lookup: ModelLookup::Answered, rev, ..was },
            Err(e) => ModelList { lookup: ModelLookup::Failed { message: e.message }, rev, ..was },
        };
        lists.set(list.clone(), answered);
        self.notify(HubNotice::ModelList(list));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::model::{AgentKind, Rev};

    fn key(cli_version: Option<&str>) -> ModelListKey {
        let cli_version = cli_version.map(|v| serde_json::from_value(v.into()).unwrap());
        ModelListKey { agent: AgentKind::Codex, repo: "r".into(), provider: "local".into(), account: None, cli_version }
    }

    fn held(lists: &ModelLists, key: &ModelListKey, lookup: ModelLookup, answered: bool) {
        let catalog = answered.then(ModelCatalog::default);
        lists.set(ModelList { key: key.clone(), catalog, lookup, rev: Rev::new(1) }, answered);
    }

    /// The machine's own CLI can be updated at any time, so its list is asked for again once it's old, and a failure
    /// waits for a person to ask again, so not every read starts the agent.
    #[tokio::test(start_paused = true)]
    async fn the_agent_is_asked_for_a_list_it_hasnt_given_one_asked_for_again_or_an_old_one_of_its_own_cli() {
        let (own, installed) = (key(None), key(Some("0.170.0")));
        let failed = ModelLookup::Failed { message: "logged out".into() };
        let asked = |lists: &ModelLists, key, again| matches!(lists.due(key, again), Due::Yes { .. });

        let lists = ModelLists::default();
        let none = (asked(&lists, &own, false), asked(&lists, &own, true));
        held(&lists, &own, ModelLookup::Asking, false);
        let asking = (asked(&lists, &own, false), asked(&lists, &own, true));
        held(&lists, &own, failed.clone(), false);
        let failing = (asked(&lists, &own, false), asked(&lists, &own, true));
        held(&lists, &own, ModelLookup::Answered, true);
        held(&lists, &installed, ModelLookup::Answered, true);
        let answered = (asked(&lists, &own, false), asked(&lists, &own, true));
        tokio::time::advance(ASK_AGAIN_AFTER).await;
        let old = (asked(&lists, &own, false), asked(&lists, &installed, false));
        let catalog = Some(ModelCatalog::default());
        lists.set(ModelList { key: own.clone(), catalog, lookup: failed, rev: Rev::new(2) }, false);
        let failed_since = lists.due(&own, true);

        assert_eq!(none, (true, true), "with no list");
        assert_eq!(asking, (false, false), "while asking already");
        assert_eq!(failing, (false, true), "once it failed");
        assert_eq!(answered, (false, true), "once it answered");
        assert_eq!(old, (true, false), "once its list is old, for its own CLI and one its provider installs");
        assert_eq!(
            failed_since,
            Due::Yes { catalog: Some(ModelCatalog::default()) },
            "it keeps the list it gave before"
        );
    }
}
