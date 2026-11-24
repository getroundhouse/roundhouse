//! A workspace's branches, which a client of version 4 reads on the workspace's record, and a client of the version
//! that reads every observation reads as an observation at the workspace's level.

use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscribe};
use protocol::client::{EVERY_OBSERVATION_FROM, HubFrame, HubNotice};
use protocol::model::{BranchName, Branches, Head, Observation, Observed, PullRequestLinked, Rev, Workspace};

use super::accounts::mark;
use super::given::{ok, start};
use crate::hub::tests::{hub_in, store_of};
use crate::server::tests::Connection;

fn on(name: &str) -> Branches {
    Branches { head: Head::Branch { name: BranchName::from(name) }, had: Vec::new() }
}

/// The workspace `w`'s record as the hub's level listed it to `client`, and what `w`'s level listed.
fn listed(client: &Connection) -> (Workspace, Vec<Observation>) {
    let workspace = client.sent.iter().find_map(|frame| {
        let HubFrame::Listed { listed: Listed::Hub(records), .. } = frame else { return None };
        records.workspaces.iter().find(|workspace| workspace.id.as_str() == "w").cloned()
    });
    let observations = client.sent.iter().find_map(|frame| {
        let HubFrame::Listed { listed: Listed::Workspace(listed), .. } = frame else { return None };
        Some(listed.observations.clone())
    });
    (workspace.expect("w listed"), observations.expect("w's level listed"))
}

/// Each copy of `w`'s record `client` was sent.
fn workspaces_sent(client: &Connection) -> Vec<Workspace> {
    let copies = client.sent.iter().filter_map(|frame| {
        let HubFrame::Notice { notice: HubNotice::Workspace(workspace), .. } = frame else { return None };
        Some(workspace.clone())
    });
    copies.collect()
}

/// Each of `w`'s branches `client` was sent as an observation.
fn observed_sent(client: &Connection) -> Vec<Observed<Branches>> {
    let copies = client.sent.iter().filter_map(|frame| {
        let HubFrame::Notice {
            notice: HubNotice::Observation(Observation::WorkspaceBranches { branches, .. }), ..
        } = frame
        else {
            return None;
        };
        Some(branches.clone())
    });
    copies.collect()
}

/// A hub that doesn't know a workspace's branches lists none either way. As its worker says them, a client of version 4
/// is sent the workspace's record again, filled, under a newer revision each time, and one of the version that reads
/// every observation is sent the observation at the workspace's level, and no copy of the record that moves only them.
/// A copy told again at the revision the last of those took reaches both.
#[tokio::test]
async fn a_workspace_s_branches_reach_each_client_in_the_form_its_version_reads() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    // What a read of its pull request with either branch finds, as following it has one read, so that changes nothing.
    store_of(&hub).set_workspace_pull_request(&"w".into(), Some(&PullRequestLinked { link: None }), None).unwrap();
    let mut before = Connection::greeted(&hub, 4).await;
    let mut observing = Connection::greeted(&hub, EVERY_OBSERVATION_FROM).await;
    let follows = vec![start(1, Follow::Hub), start(2, Follow::Workspace { id: "w".into() })];
    for client in [&mut before, &mut observing] {
        ok(client, 1, Subscribe { stop: Vec::new(), start: follows.clone() }).await;
        let both =
            |sent: &[HubFrame]| sent.iter().filter(|frame| matches!(frame, HubFrame::Listed { .. })).count() == 2;
        client.until("both lists", both).await;
    }
    let (as_listed, _) = listed(&before);
    assert_eq!(as_listed.branches, None, "not known");
    assert_eq!(listed(&observing).1, [], "not known");

    hub.branches_known(&"w".into(), on("aspen")).await;
    let moved_for_aspen = store_of(&hub).get_workspace(&"w".into()).unwrap().rev;
    hub.branches_known(&"w".into(), on("birch")).await;
    // As a task that read the record after the change announces it, at the revision its old form's copy took.
    hub.notify(HubNotice::Workspace(store_of(&hub).get_workspace(&"w".into()).unwrap()));
    for client in [&mut before, &mut observing] {
        mark(&hub, client).await;
    }

    let copies = workspaces_sent(&before);
    let branches: Vec<Option<Branches>> = copies.iter().map(|workspace| workspace.branches.clone()).collect();
    assert_eq!(branches, [Some(on("aspen")), Some(on("birch"))], "the one told again isn't newer");
    let revs: Vec<Rev> = copies.iter().map(|workspace| workspace.rev).collect();
    assert_eq!(revs[0], moved_for_aspen);
    assert!(revs[0] > as_listed.rev && revs[0] < revs[1], "{revs:?} after {:?}", as_listed.rev);
    assert_eq!(observed_sent(&before), [], "no observation to a client of version 4");
    let observed: Vec<Branches> = observed_sent(&observing)
        .into_iter()
        .map(|observed| {
            let Observed::AsOf { value, .. } = observed else { panic!("{observed:?}") };
            value
        })
        .collect();
    assert_eq!(observed, [on("aspen"), on("birch")]);
    let to_observing: Vec<Rev> = workspaces_sent(&observing).iter().map(|workspace| workspace.rev).collect();
    assert_eq!(to_observing, [revs[1]], "no copy that moves only its branches, but the one told again");
}
