//! What a client holds of a workspace's own level, as a fake hub sends it to a client that follows the workspace, and
//! the pages of its changed files past the summary.

use std::time::Duration;

use protocol::client::ObservationNotice;
use protocol::client::github::GithubShown;
use protocol::model::{
    ChangeStatus, ChangeSummary, ChangedFile, ChangedFileAt, MOST_FILES_LISTED, Observation, Observed, Rev, WorkspaceId,
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

/// A workspace a view shows in full is followed at its own level, so the client holds its changed files as the hub
/// lists them and its feed tells of them, and once it's shown no more, it holds none, and reads that the hub sends
/// none, as it does of a workspace no view showed.
#[test]
fn a_followed_workspace_s_changed_files_are_held_until_it_s_followed_no_more() {
    let mut world = World::default();
    let repo = world.repo("aspen");
    let (elm, oak) = (world.workspace(&repo, "elm"), world.workspace(&repo, "oak"));
    world.observations = vec![changes(&elm, vec![file("a.rs", ChangeStatus::Modified)], 2)];
    world.feed = vec![ObservationNotice::Copy(changes(&elm, vec![file("b.rs", ChangeStatus::Added)], 3))];
    let (_dir, client) = served(world);
    let feed = RecordsFeed::start(&client);
    let view = feed.github().view();

    view.show(GithubShown { closely: vec![elm.clone()], ..GithubShown::default() });

    let told = || paths(&feed, &elm) == Some(vec!["b.rs".to_string()]);
    waiting::thread::until(TIMEOUT, "the feed's copy of elm's files", told);
    assert_eq!(paths(&feed, &oak), None, "a workspace nothing follows");
    view.show(GithubShown::default());
    waiting::thread::until(TIMEOUT, "elm's files to be let go of", || paths(&feed, &elm).is_none());
}

/// The fake hub reads a page of a workspace's changed files past the one named by the rule a worker does, refuses one
/// of a workspace it doesn't have, and one whose world has no files, as from a worker from before the request.
#[test]
fn a_page_of_changed_files_is_read_from_the_world_by_the_worker_s_rule() {
    let mut world = World::default();
    let repo = world.repo("aspen");
    let (elm, oak) = (world.workspace(&repo, "elm"), world.workspace(&repo, "oak"));
    let files = vec![file("u.rs", ChangeStatus::Untracked), file("b.rs", ChangeStatus::Modified)];
    let reads = WorkerReads { changed_files: Some(("c1".into(), files.clone())), ..WorkerReads::default() };
    world.worker_reads.insert(elm.clone(), reads);
    let (_dir, client) = served(world);
    let after = ChangedFileAt { untracked: false, path: "a.rs".into() };

    let page = client.workspace_changed_files(&elm, after.clone()).unwrap();
    let older = client.workspace_changed_files(&oak, after.clone()).unwrap_err();
    let nowhere = client.workspace_changed_files(&"nowhere".into(), after.clone()).unwrap_err();

    let by_the_rule = protocol::model::ChangedFilesPage::after("c1".into(), files, &after, MOST_FILES_LISTED);
    assert_eq!(page, by_the_rule);
    assert!(older.is_unsupported(), "{older}");
    assert!(matches!(nowhere, ClientError::Api(refused) if refused.code == protocol::client::ErrorCode::NotFound));
}
