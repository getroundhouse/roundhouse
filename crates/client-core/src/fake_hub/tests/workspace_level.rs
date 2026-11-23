//! What a client holds of a workspace's own level, as a fake hub sends it to a client that follows the workspace, and
//! the pages of its changed files past the summary.

use std::time::Duration;

use protocol::client::ObservationNotice;
use protocol::model::{
    ChangeStatus, ChangeSummary, ChangedFile, ChangedFileAt, ChangedFilesPage, Level, Observation, Observed, Rev,
    WorkspaceId,
};

use super::super::{WorkerReads, World};
use super::served::served;
use crate::{ClientError, RecordsFeed};

const TIMEOUT: Duration = Duration::from_secs(10);

fn file(path: &str, status: ChangeStatus) -> ChangedFile {
    ChangedFile { path: path.into(), from: None, status, lines: None, version: None }
}

fn changes(workspace: &WorkspaceId, files: Vec<ChangedFile>, rev: u64) -> Observation {
    let changes = Observed::AsOf { value: ChangeSummary::new("c1".into(), files), as_of_ms: 1 };
    Observation::WorkspaceChanges { workspace_id: workspace.clone(), changes, rev: Rev::new(rev) }
}

/// The paths of the files a view reads of the workspace's changes, or `None` where the hub sends it none.
fn paths(feed: &RecordsFeed, workspace: &WorkspaceId) -> Option<Vec<String>> {
    let snapshot = feed.snapshot();
    let Observed::AsOf { value, .. } = snapshot.workspace_changes(workspace)? else { return Some(Vec::new()) };
    Some(value.files.iter().map(|file| file.path.clone()).collect())
}

/// A workspace is followed at its own level while a handle on it is kept, so the client holds its changed files as the
/// hub lists them and its feed tells of them, and once every handle on it has gone, it holds none, and reads that the
/// hub sends none, as it does of a workspace nothing followed.
#[test]
fn a_followed_workspace_s_changed_files_are_held_until_it_s_followed_no_more() {
    let mut world = World::default();
    let repo = world.repo("aspen");
    let (elm, oak) = (world.workspace(&repo, "elm"), world.workspace(&repo, "oak"));
    world.observations = vec![changes(&elm, vec![file("a.rs", ChangeStatus::Modified)], 2)];
    world.feed = vec![ObservationNotice::Copy(changes(&elm, vec![file("b.rs", ChangeStatus::Added)], 3))];
    let following = world.following.clone();
    let (_dir, client) = served(world);
    let feed = RecordsFeed::start(&client);
    let elm_followed = |level: &Level| *level == Level::Workspace { id: elm.clone() };

    let (pane, another) = (feed.follow_workspace(&elm), feed.follow_workspace(&elm));

    let told = || paths(&feed, &elm) == Some(vec!["b.rs".to_string()]);
    waiting::thread::until(TIMEOUT, "the feed's copy of elm's files", told);
    assert_eq!(paths(&feed, &oak), None, "a workspace nothing follows");
    assert_eq!(following.count(elm_followed), 1, "followed once, however many handles are kept on it");
    drop(pane);
    // Followed after the first handle has gone, so once it is, what's followed is what's wanted without that one.
    let oak_followed = feed.follow_workspace(&oak);
    waiting::thread::until(TIMEOUT, "oak to be followed", || {
        following.any(|level| *level == Level::Workspace { id: oak.clone() })
    });
    assert_eq!(following.count(elm_followed), 1, "still followed through the other handle");
    assert!(told());
    drop((another, oak_followed));
    waiting::thread::until(TIMEOUT, "elm's files to be let go of", || paths(&feed, &elm).is_none());
}

/// A workspace followed on a connection to a hub that lists a workspace's level, and then on one to a hub that takes
/// the start and lists nothing, as a hub from before a workspace's level had a list does, reads that the hub sends
/// none of its changed files, so the Changes tab reads the whole diff.
#[test]
fn a_hub_that_lists_no_workspace_level_sends_none_of_its_changed_files() {
    let world = |lists_none| {
        let mut world = World::default();
        world.lists_no_workspace_level = lists_none;
        let repo = world.repo("aspen");
        let elm = world.workspace(&repo, "elm");
        world.observations = vec![changes(&elm, vec![file("a.rs", ChangeStatus::Modified)], 2)];
        (world, elm)
    };
    let (lists, elm) = world(false);
    let (lists_none, _) = world(true);
    let following = lists_none.following.clone();
    let (_dir, client) = served(lists);
    let (_older_dir, older) = served(lists_none);
    let feed = RecordsFeed::start(&client);
    let _followed = feed.follow_workspace(&elm);
    waiting::thread::until(TIMEOUT, "elm's files", || paths(&feed, &elm).is_some());

    feed.reconnect(&older);

    waiting::thread::until(TIMEOUT, "the older hub to be followed", || following.any(|level| *level != Level::Hub));
    waiting::thread::until(TIMEOUT, "elm's files to be let go of", || paths(&feed, &elm).is_none());
}

/// The fake hub reads a page of a workspace's changed files past the one named by the rule a worker does, refuses one
/// of a workspace it doesn't have, and one whose world has no files, as from a worker from before the request.
#[test]
fn a_page_of_changed_files_is_read_from_the_world_by_the_worker_s_rule() {
    let mut world = World::default();
    let repo = world.repo("aspen");
    let (elm, oak) = (world.workspace(&repo, "elm"), world.workspace(&repo, "oak"));
    let files = vec![
        file("u.rs", ChangeStatus::Untracked),
        file("b.rs", ChangeStatus::Modified),
        file("a.rs", ChangeStatus::Modified),
    ];
    let reads = WorkerReads { changed_files: Some(("c1".into(), files)), ..WorkerReads::default() };
    world.worker_reads.insert(elm.clone(), reads);
    let (_dir, client) = served(world);
    let after = ChangedFileAt { untracked: false, path: "a.rs".into() };

    let page = client.workspace_changed_files(&elm, after.clone()).unwrap();
    let older = client.workspace_changed_files(&oak, after.clone()).unwrap_err();
    let nowhere = client.workspace_changed_files(&"nowhere".into(), after.clone()).unwrap_err();

    let tracked_then_untracked = vec![file("b.rs", ChangeStatus::Modified), file("u.rs", ChangeStatus::Untracked)];
    let by_the_rule = ChangedFilesPage { base_commit: "c1".into(), files: tracked_then_untracked, left_out: 0 };
    assert_eq!(page, by_the_rule, "those after a.rs, tracked first");
    assert!(older.is_unsupported(), "{older}");
    assert!(matches!(nowhere, ClientError::Api(refused) if refused.code == protocol::client::ErrorCode::NotFound));
}
