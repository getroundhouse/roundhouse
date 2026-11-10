use protocol::model::{PrState, PullRequest, PullRequestFound, WorkspaceId};

use super::super::tests::store_with_session;
use super::super::{Store, older, schema};

fn open() -> PullRequest {
    PullRequest {
        number: 7,
        url: "https://example.com/pull/7".to_string(),
        title: "T".to_string(),
        state: PrState::Open,
        is_draft: false,
        base: Some("main".to_string()),
        merge_state: protocol::model::MergeState::Clean,
        review: protocol::model::Review::NotRequired,
        checks: protocol::model::Checks { passed: 1, failed: 0, pending: 0 },
        queue: None,
        local: protocol::model::Local::default(),
    }
}

/// Only a change moves the revision, so the hub announces what it found only when it's news, however often it reads.
#[test]
fn finding_the_same_pull_request_again_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let id = WorkspaceId::from("w");
    let pr = open();
    let (nothing, something) = (PullRequestFound { pull_request: None }, PullRequestFound { pull_request: Some(pr) });

    let none = store.set_workspace_pull_request(&id, &nothing).unwrap();
    let none_again = store.set_workspace_pull_request(&id, &nothing).unwrap();
    let found = store.set_workspace_pull_request(&id, &something).unwrap();
    let found_again = store.set_workspace_pull_request(&id, &something).unwrap();

    let none = none.expect("looking for the first time is a change, even finding none");
    assert_eq!(none.pull_request, Some(nothing));
    assert_eq!(none_again, None);
    let found = found.unwrap();
    assert_eq!(found.pull_request, Some(something));
    assert!(found.rev > none.rev);
    assert_eq!(found_again, None);
    assert_eq!(store.get_workspace(&id).unwrap().rev, found.rev);
}

/// So a hub that starts again, after an update, still has every workspace's pull request to show.
#[test]
fn a_pull_request_found_is_kept_through_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.set_workspace_pull_request(&"w".into(), &PullRequestFound { pull_request: Some(open()) }).unwrap();
    drop(store);

    let store = Store::open(&dir.path().join("hub.db"), &dir.path().join("backups")).unwrap();

    let listed = store.list_records().unwrap().workspaces;
    assert_eq!(listed[0].pull_request, Some(PullRequestFound { pull_request: Some(open()) }));
}

#[test]
fn a_workspace_from_before_pull_requests_were_kept_has_not_been_looked_at() {
    let dir = tempfile::tempdir().unwrap();

    let store = older::store_from_before(dir.path(), schema::BEFORE_PULL_REQUESTS, "");

    assert_eq!(store.get_workspace(&"w".into()).unwrap().pull_request, None);
}
