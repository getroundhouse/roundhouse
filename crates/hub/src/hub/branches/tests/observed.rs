//! A workspace's branches, which the hub keeps as an observation: current while its worker watches them, and as of when
//! it went after, across a restart as of their read, not known from a database from before, and forgotten with the
//! workspace, as it's archived, and with what the database lacks.

use std::sync::Arc;

use protocol::model::{Branches, Head, Level, Observation, Observed, WorkspaceId};
use protocol::worker::WorkerFrame;
use rusqlite::Connection;

use crate::hub::Hub;
use crate::hub::observations::{ChildWorktreeBranches, ChildWorktreeKey, WorkspaceBranches};
use crate::hub::tests::{hub_in_again, hub_with_worktree, taking_worker};
use crate::store::BEFORE_BRANCHES_OBSERVED;
use crate::store::columns::OneValue;
use crate::store::now_ms;
use crate::store::older::store_from_before;
use crate::worker_link::WorkerLink;

fn on(name: &str) -> Branches {
    Branches { head: Head::Branch { name: name.into() }, had: Vec::new() }
}

/// What the workspace `w`'s level lists of its branches.
async fn listed(hub: &Hub) -> Option<Observed<Branches>> {
    let (listed, _) = hub.observations_at(&Level::Workspace { id: "w".into() }).await.unwrap();
    listed.into_iter().find_map(|observation| {
        let Observation::WorkspaceBranches { branches, .. } = observation else { return None };
        Some(branches)
    })
}

/// Has the hub take in that `link`, `w`'s worker, has gone, as it does once the worker's stream ends.
async fn gone(hub: &Hub, link: &Arc<WorkerLink>) {
    let workspace = hub.workspace(&"w".into()).await.unwrap();
    let provider = hub.provider(&workspace.provider).unwrap();
    hub.worker_gone(&"w".into(), link, provider.as_ref(), &workspace.instance_id).await;
}

/// Its worker says every change while it watches them, so they're current until it goes, and as of then after.
#[tokio::test]
async fn branches_are_current_while_the_worker_watches_and_as_of_when_it_went() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let (link, _) = taking_worker(&hub);
    let ready = hub.ready_workspace(&"w".into()).await.unwrap();

    hub.watch_branches(&ready, &link).await;
    hub.branches_frame(&"w".into(), &link, WorkerFrame::Branches { branches: on("aspen") }).await;
    let watched = listed(&hub).await;
    let before = now_ms();
    gone(&hub, &link).await;
    let after_it = listed(&hub).await;

    assert_eq!(watched, Some(Observed::Current { value: on("aspen") }));
    let Some(Observed::AsOf { value, as_of_ms }) = after_it else { panic!("{after_it:?}") };
    assert_eq!(value, on("aspen"));
    assert!(as_of_ms >= before, "as of when the worker went, not of the read");
}

/// A hub that starts again has them as of their read, until a worker says them again.
#[tokio::test]
async fn branches_are_kept_across_a_restart_as_of_their_read() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let before = now_ms();
    hub.branches_known(&"w".into(), on("aspen")).await;
    hub.observations_written().await;
    drop(hub);

    let again = hub_in_again(dir.path());

    let Some(Observed::AsOf { value, as_of_ms }) = listed(&again).await else { panic!("not as of a read") };
    assert_eq!(value, on("aspen"));
    assert!(as_of_ms >= before);
    assert_eq!(again.workspace(&"w".into()).await.unwrap().branches, None, "not on the record the hub reads");
}

/// A database from before keeps its workspaces and their links, with their branches gone and not known, and with no
/// count of uncommitted files on a link, so no hub reads either from it.
#[tokio::test]
async fn a_workspace_from_before_opens_with_its_branches_not_known_and_its_link_kept() {
    let dir = tempfile::tempdir().unwrap();
    let rows = r#"UPDATE workspaces SET branches = '{"head":{"kind":"branch","name":"aspen"},"had":[]}',
            pull_request_link = '{"repository":"o/r","number":7,"url":"https://github.com/o/r/pull/7","uncommitted":2}',
            pull_request_branch = 'aspen'
            WHERE id = 'w';"#;
    drop(store_from_before(dir.path(), BEFORE_BRANCHES_OBSERVED, rows));

    let hub = hub_in_again(dir.path());

    let db = Connection::open(dir.path().join("hub.db")).unwrap();
    let columns = |table: &str| -> Vec<String> {
        let mut statement = db.prepare(&format!("SELECT name FROM pragma_table_info('{table}')")).unwrap();
        statement.query_map([], OneValue::read).unwrap().map(Result::unwrap).collect()
    };
    assert!(!columns("workspaces").contains(&"branches".to_string()));
    assert!(!columns("child_worktrees").contains(&"branches".to_string()));
    let stored: String =
        db.query_row("SELECT pull_request_link FROM workspaces WHERE id = 'w'", [], OneValue::read).unwrap();
    assert_eq!(stored, r#"{"repository":"o/r","number":7,"url":"https://github.com/o/r/pull/7"}"#);
    let workspace = hub.workspace(&"w".into()).await.unwrap();
    assert_eq!(workspace.pull_request_linked().map(|link| link.number.get()), Some(7));
    assert_eq!(workspace.pull_request_branch.as_ref().map(ToString::to_string).as_deref(), Some("aspen"));
    assert_eq!(listed(&hub).await, None, "not known");
    assert_eq!(hub.branches_of(&"w".into()), None);
}

/// Archiving forgets the workspace's own, as `crate::hub::workspace_lines`' tests show, and its other worktrees' stay,
/// since a branch one had is still the workspace's. Removing it forgets both, and a hub that starts on a database
/// without a workspace or a worktree forgets theirs.
#[tokio::test]
async fn branches_are_forgotten_as_the_workspace_is_removed_and_with_what_the_database_lacks() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let child = |workspace_id: &str, id: &str| ChildWorktreeKey { workspace_id: workspace_id.into(), id: id.into() };
    hub.branches_known(&"w".into(), on("aspen")).await;
    hub.child_branches_known(&"w".into(), "c".into(), on("alder/x")).await;

    hub.branches_forgotten(&"w".into()).await;
    let archiving = (hub.branches_of(&"w".into()), hub.observations.keeps::<ChildWorktreeBranches>(&child("w", "c")));
    hub.branches_known(&"gone".into(), on("birch")).await;
    hub.child_branches_known(&"gone".into(), "d".into(), on("birch/x")).await;
    hub.workspace_observations_gone(&"w".into()).await;
    let removing = hub.observations.keeps::<ChildWorktreeBranches>(&child("w", "c"));
    hub.observations_written().await;
    drop(hub);
    let again = hub_in_again(dir.path());

    assert_eq!(archiving, (None, true));
    assert!(!removing);
    assert!(!again.observations.keeps::<WorkspaceBranches>(&WorkspaceId::from("gone")), "no such workspace");
    assert!(!again.observations.keeps::<ChildWorktreeBranches>(&child("gone", "d")), "no such worktree");
}
