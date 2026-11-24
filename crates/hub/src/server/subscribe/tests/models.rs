//! What an agent offers where a chat would launch, which a client of version 4 reads on its list of models' record,
//! and a client of the version that reads every observation reads as an observation.

use protocol::capability::settings::{Effort, ModelCatalog};
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{EVERY_OBSERVATION_FROM, HubFrame, HubNotice};
use protocol::model::{AgentKind, ModelListKey, Observation};

use super::accounts::mark;
use super::given::{ok, start};
use crate::hub::tests::hub_in;
use crate::server::tests::Connection;

/// A list's agent answering what it offers sends a client of version 4 the list's record with it, and no observation,
/// and sends one of the version that reads every observation the observation.
#[tokio::test]
async fn what_an_agent_offers_reaches_each_client_in_the_form_its_version_reads() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut before = Connection::greeted(&hub, 4).await;
    let mut observing = Connection::greeted(&hub, EVERY_OBSERVATION_FROM).await;
    for client in [&mut before, &mut observing] {
        ok(client, 1, Subscribe { stop: Vec::new(), start: vec![start(1, Follow::Hub)] }).await;
        client.until("the list", |sent| sent.iter().any(|frame| matches!(frame, HubFrame::Listed { .. }))).await;
    }
    let key = ModelListKey {
        agent: AgentKind::Codex,
        repo: "r".into(),
        provider: "local".into(),
        account: None,
        cli_version: None,
    };
    let offered = ModelCatalog {
        offered: vec![protocol::fixtures::offer("orchard", "Orchard", &[Effort::Low], Effort::Low)],
        default_model: None,
    };

    hub.model_list_answered(&key, offered.clone()).await;

    for client in [&mut before, &mut observing] {
        mark(&hub, client).await;
    }
    let catalogs_on_the_list = |sent: &[HubFrame]| {
        let each = sent.iter().filter_map(|frame| {
            let HubFrame::Notice { notice: HubNotice::ModelList(list), .. } = frame else { return None };
            list.catalog.clone()
        });
        each.collect::<Vec<_>>()
    };
    let observed = |sent: &[HubFrame]| {
        sent.iter().any(|frame| {
            matches!(frame, HubFrame::Notice { notice: HubNotice::Observation(Observation::ModelCatalog { list, .. }), .. } if *list == key)
        })
    };
    assert_eq!(catalogs_on_the_list(&before.sent), [offered], "its record, filled");
    assert!(!observed(&before.sent), "no observation to a client of version 4");
    assert!(observed(&observing.sent));
}
