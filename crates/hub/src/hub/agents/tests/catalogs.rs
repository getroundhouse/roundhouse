//! What an agent offers where a chat would launch, which the hub keeps as an observation, and fills each list of models'
//! record with for a client of a version from before it.

use std::sync::Arc;
use std::time::Duration;

use protocol::capability::settings::{Effort, ModelCatalog};
use protocol::model::{AgentKind, ModelList, ModelListKey, ModelLookup, Observation, Observed, RepoOrigin};

use super::super::model_lists::Lookup;
use crate::hub::Hub;
use crate::hub::tests::{hub_in, hub_in_again};

/// The list of Codex's models in `r`, on the local provider, of the CLI of `cli_version`, or the machine's own.
fn key(cli_version: Option<&str>) -> ModelListKey {
    let cli_version = cli_version.map(|v| serde_json::from_value(v.into()).unwrap());
    ModelListKey { agent: AgentKind::Codex, repo: "r".into(), provider: "local".into(), account: None, cli_version }
}

/// What Codex offers, as one model.
fn offered() -> ModelCatalog {
    ModelCatalog {
        offered: vec![protocol::fixtures::offer("orchard", "Orchard", &[Effort::Low], Effort::Low)],
        default_model: None,
    }
}

/// Where the agent is asked for the list at `key`.
fn lookup(key: &ModelListKey) -> Lookup {
    Lookup { key: key.clone(), origin: RepoOrigin::Checkout { path: "/repo".into() } }
}

/// What `hub` lists of what's offered at `key`, and the record of its list, if it has one.
async fn listed(hub: &Hub, key: &ModelListKey) -> (Option<Observed<ModelCatalog>>, Option<ModelList>) {
    let records = hub.records_list().await.unwrap();
    let observed = records.observations.into_iter().find_map(|observation| {
        let Observation::ModelCatalog { list, catalog, .. } = observation else { return None };
        (list == *key).then_some(catalog)
    });
    (observed, records.model_lists.into_iter().find(|list| list.key == *key))
}

/// A hub that starts again has what an agent offered as of its answer, with no list held, since a list is held only
/// while the hub has been asked for it. Asked again, one of a CLI a provider installed, whose version says what it
/// offers, is answered from it at once, and the agent isn't asked, and its list's record holds it.
#[tokio::test]
async fn what_an_agent_offered_is_kept_across_a_restart_and_serves_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let installed = key(Some("0.170.0"));
    let hub = hub_in(dir.path());
    hub.model_list_answered(&installed, offered()).await;
    hub.observations_written().await;
    drop(hub);

    let again = hub_in_again(dir.path());

    let (observed, list) = listed(&again, &installed).await;
    assert!(matches!(&observed, Some(Observed::AsOf { value, .. }) if *value == offered()), "{observed:?}");
    assert_eq!(list, None, "no list held");
    assert_eq!(again.model_catalog(lookup(&installed)).await.unwrap(), offered());
    let (_, list) = listed(&again, &installed).await;
    let list = list.expect("held once asked for");
    assert_eq!((list.catalog, list.lookup), (Some(offered()), ModelLookup::Answered), "filled, and not asked");
}

/// One the machine's own CLI offered, which can change whenever its machine's CLI is updated, serves at once too, and
/// the agent is asked again meanwhile.
#[tokio::test]
async fn what_the_machine_s_own_cli_offered_serves_at_once_and_is_asked_again() {
    let dir = tempfile::tempdir().unwrap();
    let own = key(None);
    let hub = hub_in(dir.path());
    hub.model_list_answered(&own, offered()).await;
    hub.observations_written().await;
    drop(hub);
    let again = hub_in_again(dir.path());

    let catalog = again.model_catalog(lookup(&own)).await;

    assert_eq!(catalog.unwrap(), offered());
    let (_, list) = listed(&again, &own).await;
    let list = list.expect("held once asked for");
    assert!(!matches!(list.lookup, ModelLookup::Answered), "asked again: {:?}", list.lookup);
    assert_eq!(list.catalog, Some(offered()), "what it offered before, meanwhile");
}

/// Letting go of a list lets go of what its agent offered, and of what the cache file kept of one no list holds, so a
/// list a change of login made wrong isn't answered from it.
#[tokio::test]
async fn letting_go_of_a_list_lets_go_of_what_was_offered_there() {
    let dir = tempfile::tempdir().unwrap();
    let (held, kept) = (key(Some("0.170.0")), key(Some("0.171.0")));
    let hub = hub_in(dir.path());
    hub.model_list_answered(&kept, offered()).await;
    hub.observations_written().await;
    drop(hub);
    let again = hub_in_again(dir.path());
    again.model_list_answered(&held, offered()).await;

    again.forget_model_lists(|_| true).await;

    assert_eq!(listed(&again, &held).await, (None, None));
    assert_eq!(listed(&again, &kept).await, (None, None), "the file's");
}

/// An answer that lands as the hub lets go of its list, as when which login a launch is handed changes, is another
/// login's, so it isn't kept, though it found its list held as it began to, and doesn't answer a list made in its
/// place since. Otherwise the next to ask there would be served it at once, as one the cache file kept.
#[tokio::test]
async fn an_answer_that_lands_as_its_list_is_let_go_of_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let installed = key(Some("0.170.0"));
    let hub = hub_in(dir.path());
    let asked = hub.model_list_asking(&installed).await;
    // A change to an observation under way, which the answer waits behind.
    let under_way = hub.holding_observations().await;
    let answer = tokio::spawn({
        let (hub, key) = (Arc::clone(&hub), installed.clone());
        async move { hub.model_list_settled(&key, asked, Ok(offered())).await }
    });
    // It runs up to the lock it waits for, on the test's one thread.
    tokio::task::yield_now().await;
    let forgetting = tokio::spawn({
        let hub = Arc::clone(&hub);
        async move { hub.forget_model_lists(|_| true).await }
    });
    waiting::task::until(Duration::from_secs(5), "the list let go of", || hub.model_list_held(&installed).is_none())
        .await;
    let again = hub.model_list_asking(&installed).await;

    drop(under_way);
    answer.await.unwrap();
    forgetting.await.unwrap();

    assert_eq!(listed(&hub, &installed).await.0, None, "nothing kept of what it offered");
    let list = hub.model_list_held(&installed).expect("the list made since");
    assert_eq!((list.rev, list.lookup), (again, ModelLookup::Asking), "still asking");
}

/// A lookup has said what it found a moment before its job ends, so a person can ask again in between, as Retry does
/// once a list has failed. The agent is asked again once that job has ended, rather than the list failing as though the
/// hub were asking about as many places as it can.
#[tokio::test]
async fn a_list_asked_again_as_its_failed_lookup_ends_is_asked_for_once_it_has() {
    let dir = tempfile::tempdir().unwrap();
    let installed = key(Some("0.170.0"));
    let hub = hub_in(dir.path());
    let asked = hub.model_list_asking(&installed).await;
    let ending = hub.model_lookup_ending(&installed);
    hub.model_list_settled(&installed, asked, Err("logged out")).await;

    let again = hub.ensure_model_list(lookup(&installed), true).await.unwrap();
    drop(ending);

    assert_eq!(again.lookup, ModelLookup::Asking);
    waiting::task::until(Duration::from_secs(5), "the agent asked again", || {
        hub.model_list_held(&installed).is_some_and(|list| list.rev > again.rev)
    })
    .await;
}
