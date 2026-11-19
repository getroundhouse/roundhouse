use protocol::model::{
    BranchName, Branches, DiffComment, DiffSide, Head, PrState, PullRequest, PullRequestFound, Repo, Rev, Session,
    Workspace, WorkspaceId, WorkspaceStatus,
};

use super::super::tests::{idle_session, store_with_session, workspace_w};
use super::super::{Ended, Made, Move, WorkspacePlacement, now_ms};
use super::super::{Store, StoredSession, older, schema};

fn open() -> PullRequest {
    PullRequest {
        number: 7,
        url: "https://example.com/pull/7".to_string(),
        title: "T".to_string(),
        state: PrState::Open,
        is_draft: false,
        base: Some("main".into()),
        merge_state: protocol::model::MergeState::Clean,
        review: protocol::model::Review::NotRequired,
        checks: protocol::model::Checks { passed: 1, failed: 0, pending: 0 },
        queue: None,
        left_queue: None,
        changes_requested_at_ms: None,
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

    let none = store.set_workspace_pull_request(&id, &nothing, None).unwrap();
    let none_again = store.set_workspace_pull_request(&id, &nothing, None).unwrap();
    let found = store.set_workspace_pull_request(&id, &something, None).unwrap();
    let found_again = store.set_workspace_pull_request(&id, &something, None).unwrap();

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
    store.set_workspace_pull_request(&"w".into(), &PullRequestFound { pull_request: Some(open()) }, None).unwrap();
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

#[test]
fn workspace_names_are_unique_per_repo() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mut duplicate = store.get_workspace(&"w".into()).unwrap();
    duplicate.id = "w2".into();
    assert_eq!(store.insert_workspace(&duplicate, &WorkspacePlacement::local_checkout("/repo"), None).unwrap(), None);
}

/// A workspace on its way keeps its ID through a new name, so every client's note for it stays where it is.
#[test]
fn only_a_workspace_being_made_takes_a_new_name_and_only_one_nobody_else_has() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let making = WorkspaceId::from("making");
    let before = workspace_w(now_ms());
    let workspace = protocol::model::Workspace {
        id: making.clone(),
        name: "birch".into(),
        branch: "birch".into(),
        instance_id: "making".into(),
        status: WorkspaceStatus::Creating,
        ..before.clone()
    };
    let Some(Made::New(made)) =
        store.insert_workspace(&workspace, &WorkspacePlacement::local_checkout("/repo"), None).unwrap()
    else {
        panic!("not made")
    };

    let renamed = store.rename_unready_workspace(&making, "cedar", &"cedar/start".into()).unwrap().unwrap();
    let taken = store.rename_unready_workspace(&making, "alder", &"alder/start".into()).unwrap();
    let ready = store.rename_unready_workspace(&before.id, "elm", &"elm/start".into()).unwrap();

    assert_eq!((renamed.name.as_str(), renamed.branch.as_str()), ("cedar", "cedar/start"));
    assert!(renamed.rev > made.rev);
    assert_eq!(taken, None, "w has it");
    assert_eq!(ready, None, "w has a worktree by its name");
    assert_eq!(store.get_workspace(&making).unwrap().name, "cedar");
    assert_eq!(store.get_workspace(&before.id).unwrap().name, "alder");
}

/// So it's known whose it is, when the worktree's on another branch by the time it's next looked at.
#[test]
fn finding_the_same_pull_request_on_another_branch_records_that_branch() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let id = WorkspaceId::from("w");
    let found = PullRequestFound { pull_request: Some(open()) };
    let (aspen, header) = (BranchName::from("aspen"), BranchName::from("header"));
    store.set_workspace_pull_request(&id, &found, Some(&aspen)).unwrap();

    let same = store.set_workspace_pull_request(&id, &found, Some(&aspen)).unwrap();
    let elsewhere = store.set_workspace_pull_request(&id, &found, Some(&header)).unwrap();

    assert_eq!(same, None);
    assert_eq!(elsewhere.unwrap().pull_request_branch, Some(header));
}

/// The worker sends them whenever git's files change, and only a change to them is news.
#[test]
fn reading_the_same_branches_again_changes_nothing_and_says_what_they_were() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let id = WorkspaceId::from("w");
    let on = |name: &str| Branches { head: Head::Branch { name: BranchName::from(name) }, had: Vec::new() };

    let first = store.set_workspace_branches(&id, &on("aspen")).unwrap();
    let again = store.set_workspace_branches(&id, &on("aspen")).unwrap();
    let switched = store.set_workspace_branches(&id, &on("header")).unwrap();

    let read_first = first.workspace.expect("the first read is a change");
    assert_eq!((first.before, read_first.branches), (None, Some(on("aspen"))));
    assert_eq!((again.before, again.workspace), (Some(on("aspen")), None));
    assert_eq!(switched.before, Some(on("aspen")));
    assert!(switched.workspace.unwrap().rev > read_first.rev);
}

/// Starting is what keeps a second request from archiving or restoring it too, and putting it back leaves it as it was,
/// when it was archived included.
#[test]
fn a_workspace_is_moved_by_one_request_at_a_time_and_put_back_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let id = WorkspaceId::from("w");
    let ready = store.get_workspace(&id).unwrap();

    let archiving = store.start_move(&id, Move::Archive).unwrap().unwrap();
    let twice = store.start_move(&id, Move::Archive).unwrap();
    let restoring_too = store.start_move(&id, Move::Restore).unwrap();
    let put_back = store.end_move(&id, Move::Archive, Ended::PutBack).unwrap();

    assert_eq!(archiving.status, WorkspaceStatus::Archiving);
    assert!(archiving.rev > ready.rev, "so the feed tells every client");
    assert_eq!((twice, restoring_too), (None, None));
    assert_eq!((put_back.status, put_back.archived_at_ms), (WorkspaceStatus::Ready, None));
    assert!(put_back.rev > archiving.rev);

    store.start_move(&id, Move::Archive).unwrap().unwrap();
    let archived = store.end_move(&id, Move::Archive, Ended::Done).unwrap();
    let restoring = store.start_move(&id, Move::Restore).unwrap().unwrap();
    let not_restored = store.end_move(&id, Move::Restore, Ended::PutBack).unwrap();
    store.start_move(&id, Move::Restore).unwrap().unwrap();
    let restored = store.end_move(&id, Move::Restore, Ended::Done).unwrap();

    let archived_at = archived.archived_at_ms;
    assert_eq!(archived.status, WorkspaceStatus::Archived);
    assert!(archived_at.is_some());
    assert_eq!((restoring.status, restoring.archived_at_ms), (WorkspaceStatus::Restoring, archived_at));
    assert_eq!((not_restored.status, not_restored.archived_at_ms), (WorkspaceStatus::Archived, archived_at));
    assert_eq!((restored.status, restored.archived_at_ms), (WorkspaceStatus::Ready, None));
}

/// Putting back only undoes what the move it's for did.
#[test]
fn putting_back_leaves_a_workspace_the_move_isnt_holding() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    let left = store.end_move(&"w".into(), Move::Restore, Ended::PutBack).unwrap();

    assert_eq!(left.status, WorkspaceStatus::Ready, "it wasn't being restored, so it isn't put back as archived");
}

/// Each list is in the order `protocol` orders its kind of record in, which a client keeps and a fake hub lists them in,
/// with records made in another order, and two made at the same moment, so the IDs decide.
#[test]
fn records_are_listed_in_the_order_protocol_orders_them() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    for (id, name) in [("r-b", "birch"), ("r-a", "aspen"), ("r-0", "aspen")] {
        store.add_repo(&id.into(), name, &format!("/{id}"), 0).unwrap();
    }
    for (id, at) in [("w-b", 5), ("w-a", 5), ("w-c", 1)] {
        store
            .insert_workspace(
                &Workspace { created_at_ms: at, ..protocol::fixtures::workspace(id) },
                &WorkspacePlacement::local_checkout("/repo"),
                None,
            )
            .unwrap();
        let session = idle_session(at);
        let session = StoredSession { session: Session { id: format!("s-{id}").into(), ..session.session }, ..session };
        store.insert_session(&session, None).unwrap();
        let comment = DiffComment {
            id: format!("c-{id}").into(),
            workspace_id: "w".into(),
            path: "a.rs".into(),
            side: DiffSide::New,
            line: 1,
            text: "why?".into(),
            created_at_ms: at,
            rev: Rev::default(),
        };
        store.add_comment(&comment, None).unwrap();
    }

    let listed = store.list_records().unwrap();
    let (repos, workspaces) = (store.list_repos().unwrap(), store.list_workspaces(true).unwrap());
    let sessions = store.list_sessions(&"w".into()).unwrap();

    assert_eq!(listed.repos, in_order(&listed.repos, Repo::by_name));
    assert_eq!(repos, listed.repos);
    assert_eq!(listed.workspaces, in_order(&listed.workspaces, Workspace::oldest_first));
    assert_eq!(workspaces, listed.workspaces);
    assert_eq!(listed.sessions, in_order(&listed.sessions, Session::oldest_first));
    assert_eq!(sessions, listed.sessions);
    assert_eq!(listed.comments, in_order(&listed.comments, DiffComment::oldest_first));
    let ids: Vec<&str> = listed.workspaces.iter().map(|workspace| workspace.id.as_str()).collect();
    assert_eq!(ids, ["w-c", "w-a", "w-b", "w"], "made out of order, two at once");
}

fn in_order<T: Clone>(records: &[T], order: impl Fn(&T, &T) -> std::cmp::Ordering) -> Vec<T> {
    let mut ordered = records.to_vec();
    ordered.sort_by(order);
    ordered
}
