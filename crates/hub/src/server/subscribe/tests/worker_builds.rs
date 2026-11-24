//! Which build a workspace's worker is, which a client of version 4 reads on the workspace's record, and a client of the
//! version that reads every observation reads as an observation at the hub's level, kept in memory alone.

use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{EVERY_OBSERVATION_FROM, HubFrame, HubNotice};
use protocol::model::{Observation, ObservationKey, Observed, OtherBuild, Workspace};
use rusqlite::Connection as Database;

use super::accounts::mark;
use super::given::{ok, start};
use crate::hub::tests::{hub_in, hub_in_again};
use crate::server::tests::Connection;
use crate::store::BEFORE_WORKER_BUILD_OBSERVED;
use crate::store::columns::OneValue;
use crate::store::older::store_from_before;
use crate::worker_link::WorkerLink;

/// What `client` was sent of `w`'s record, listed and then each copy, as which build its worker is.
fn builds_sent(client: &Connection) -> Vec<Option<String>> {
    let copies = client.sent.iter().flat_map(|frame| {
        if let HubFrame::Listed { listed: Listed::Hub(records), .. } = frame {
            return records.workspaces.clone();
        }
        let HubFrame::Notice { notice: HubNotice::Workspace(workspace), .. } = frame else { return Vec::new() };
        vec![workspace.clone()]
    });
    copies
        .filter(|workspace: &Workspace| workspace.id.as_str() == "w")
        .map(|workspace| workspace.worker_build)
        .collect()
}

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

/// As the hub carries on with a worker of another build, a client of version 4 is sent the workspace's record with its
/// build, and again without once it's gone, and one of the version that reads every observation is sent the
/// observation, current, and its removal, and no copy of the record that moves only it. A worker too old to name its
/// build is another all the same, which only the observation can say.
#[tokio::test]
async fn which_build_a_worker_is_reaches_each_client_in_the_form_its_version_reads() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let mut before = Connection::greeted(&hub, 4).await;
    let mut observing = Connection::speaking(&hub, EVERY_OBSERVATION_FROM);
    for client in [&mut before, &mut observing] {
        ok(client, 1, Subscribe { stop: Vec::new(), start: vec![start(1, Follow::Hub)] }).await;
        client.until("the list", |sent| sent.iter().any(|frame| matches!(frame, HubFrame::Listed { .. }))).await;
    }

    hub.worker_build_heard(&"w".into(), &WorkerLink::unconnected().of_build("0.9.0")).await;
    hub.worker_build_gone(&"w".into()).await;
    hub.worker_build_heard(&"w".into(), &WorkerLink::unconnected()).await;
    for client in [&mut before, &mut observing] {
        mark(&hub, client).await;
    }

    // The last is the one too old to name its build, which this field can't tell from the hub's own.
    assert_eq!(builds_sent(&before), [None, Some("0.9.0".into()), None, None]);
    let said = |said: Option<&str>| Some(Observed::Current { value: OtherBuild { said: said.map(str::to_string) } });
    assert_eq!(observed_sent(&observing), [said(Some("0.9.0")), None, said(None)]);
    assert_eq!(observed_sent(&before), [], "no observation to a client of version 4");
    assert_eq!(builds_sent(&observing), [None], "listed, and no copy that moves only its build");
}

/// It's kept in memory alone, so a hub that starts again knows no worker's build until it carries on with one, and a
/// database from before it left the record opens with the column gone and the workspace as it was.
#[tokio::test]
async fn which_build_a_worker_is_is_not_known_after_a_restart_nor_from_a_database_from_before() {
    let dir = tempfile::tempdir().unwrap();
    drop(store_from_before(dir.path(), BEFORE_WORKER_BUILD_OBSERVED, "UPDATE workspaces SET worker_build = '0.9.0';"));
    let hub = hub_in_again(dir.path());
    hub.worker_build_heard(&"w".into(), &WorkerLink::unconnected().of_build("0.9.0")).await;
    hub.observations_written().await;
    drop(hub);

    let again = hub_in_again(dir.path());

    let listed = again.records_list().await.unwrap();
    assert!(!listed.observations.iter().any(|told| matches!(told, Observation::WorkspaceWorkerBuild { .. })));
    let [workspace] = listed.workspaces.as_slice() else { panic!("{:?}", listed.workspaces) };
    assert_eq!((workspace.name.as_str(), workspace.worker_build.as_ref()), ("alder", None));
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
