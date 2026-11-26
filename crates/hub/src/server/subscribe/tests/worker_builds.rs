//! Which build a workspace's worker is, an observation at the hub's level, kept in memory alone.

use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{CLIENT_PROTOCOL_VERSION, HubFrame, HubNotice};
use protocol::model::{Observation, ObservationKey, Observed, OtherBuild};
use rusqlite::Connection as Database;

use super::given::{mark, ok, start};
use crate::hub::Keeper;
use crate::hub::tests::{hub_in, hub_in_again, serving};
use crate::server::tests::Connection;
use crate::store::BEFORE_WORKER_BUILD_OBSERVED;
use crate::store::columns::OneValue;
use crate::store::older::store_from_before;
use crate::worker_link::WorkerLink;

/// What `client` was told of `w`'s worker's build as an observation: each copy, and `None` for its removal.
fn observed_sent(client: &Connection) -> Vec<Option<Observed<OtherBuild>>> {
    let gone = ObservationKey::WorkspaceWorkerBuild { workspace_id: "w".into() };
    let told = client.sent.iter().filter_map(|frame| {
        let HubFrame::Notice { notice, .. } = frame else { return None };
        if let HubNotice::Observation(Observation::WorkspaceWorkerBuild { build, .. }) = notice {
            return Some(Some(build.clone()));
        }
        let HubNotice::ObservationRemoved { observation, .. } = notice else { return None };
        (*observation == gone).then_some(None)
    });
    told.collect()
}

/// As the hub carries on with a worker of another build, a client is sent the observation, current, and its removal
/// once it's gone. A worker too old to name its build is another all the same.
#[tokio::test]
async fn which_build_a_worker_is_is_sent_as_an_observation_and_its_removal() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut client = Connection::greeted(&hub, CLIENT_PROTOCOL_VERSION).await;
    ok(&mut client, 1, Subscribe { stop: Vec::new(), start: vec![start(1, Follow::Hub)] }).await;
    client.until("the list", |sent| sent.iter().any(|frame| matches!(frame, HubFrame::Listed { .. }))).await;

    let (first, next) = (WorkerLink::unconnected().of_build("0.9.0"), WorkerLink::unconnected());
    serving(&hub, &first);
    hub.worker_build_heard(&"w".into(), &first).await;
    hub.keeper_went(Keeper::Worker(first.id)).await;
    serving(&hub, &next);
    hub.worker_build_heard(&"w".into(), &next).await;
    mark(&hub, &mut client).await;

    let said = |said: Option<&str>| Some(Observed::Current { value: OtherBuild { said: said.map(str::to_string) } });
    assert_eq!(observed_sent(&client), [said(Some("0.9.0")), None, said(None)]);
}

/// It's kept in memory alone, so a hub that starts again knows no worker's build until it carries on with one, and a
/// database from before it left the record opens with the column gone and the workspace as it was.
#[tokio::test]
async fn which_build_a_worker_is_is_not_known_after_a_restart_nor_from_a_database_from_before() {
    let dir = tempfile::tempdir().unwrap();
    drop(store_from_before(dir.path(), BEFORE_WORKER_BUILD_OBSERVED, "UPDATE workspaces SET worker_build = '0.9.0';"));
    let hub = hub_in_again(dir.path());
    let link = WorkerLink::unconnected().of_build("0.9.0");
    serving(&hub, &link);
    hub.worker_build_heard(&"w".into(), &link).await;
    hub.observations_written().await;
    drop(hub);

    let again = hub_in_again(dir.path());

    let listed = again.records_list().await.unwrap();
    assert!(!listed.observations.iter().any(|told| matches!(told, Observation::WorkspaceWorkerBuild { .. })));
    let [workspace] = listed.workspaces.as_slice() else { panic!("{:?}", listed.workspaces) };
    assert_eq!(workspace.name.as_str(), "alder");
    let columns: Vec<String> = {
        let db = Database::open(dir.path().join("hub.db")).unwrap();
        let mut statement = db.prepare("SELECT name FROM pragma_table_info('workspaces')").unwrap();
        statement.query_map([], OneValue::read).unwrap().map(Result::unwrap).collect()
    };
    assert!(!columns.contains(&"worker_build".to_string()), "{columns:?}");
    let cache = Database::open(dir.path().join("github-cache.db")).unwrap();
    let tables: Vec<String> = cache
        .prepare("SELECT name FROM sqlite_schema WHERE name LIKE '%worker_build%'")
        .unwrap()
        .query_map([], OneValue::read)
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(tables, [] as [String; 0], "no table in the cache file");
}
