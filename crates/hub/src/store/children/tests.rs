use protocol::fixtures;
use protocol::model::{BranchName, Branches, ChildStatus, ChildWorktree, Head};

use crate::store::tests::{idle_session, store_with_session};
use crate::store::{Made, current_rev, lock};

/// One of `w`'s, made by `s`'s agent, as the hub records it before asking for it.
fn child(id: &str, path: &str) -> ChildWorktree {
    ChildWorktree {
        path: path.into(),
        branch: BranchName::from("alder/x"),
        created_at_ms: 1,
        status: ChildStatus::Creating,
        ..fixtures::child_worktree(id)
    }
}

/// So a hub that hears the ask again, as each new hub does, makes the worktree once, and two worktrees there never
/// share a path, though one removed keeps its record beside the next made there.
#[test]
fn an_ask_makes_one_record_and_a_path_is_one_worktree_s_until_it_s_removed() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    let first = store.insert_child(&child("a", "/c/x"), &"k1".into()).unwrap();
    let again = store.insert_child(&child("b", "/c/y"), &"k1".into()).unwrap();
    let same_path = store.insert_child(&child("c", "/c/x"), &"k2".into()).unwrap();
    store.child_ready(&"a".into(), None).unwrap();
    let removed = store.child_removed(&"a".into()).unwrap();
    let after = store.insert_child(&child("d", "/c/x"), &"k3".into()).unwrap();
    let mut other = idle_session(1);
    other.session.id = "s2".into();
    store.insert_session(&other, None).unwrap();
    let elsewhere = store.insert_child(&ChildWorktree { session_id: "s2".into(), ..child("e", "/c/z") }, &"k1".into());

    let Some(Made::New(first)) = first else { panic!("not made: {first:?}") };
    assert_eq!(first.status, ChildStatus::Creating);
    assert_eq!(again, Some(Made::Earlier(first.clone())), "the record the ask made");
    assert_eq!(same_path, None, "a worktree is there");
    assert!(matches!(removed.status, ChildStatus::Removed { .. }));
    assert!(matches!(after, Some(Made::New(_))), "a removed one's path is free");
    assert!(matches!(elsewhere.unwrap(), Some(Made::New(_))), "another session's key is its own");
    assert_eq!(store.child_for_ask(&"s2".into(), &"k1".into()).unwrap().map(|c| c.id), Some("e".into()));
    let ids = |children: Vec<ChildWorktree>| children.into_iter().map(|child| child.id).collect::<Vec<_>>();
    assert_eq!(ids(store.children_of(&"w".into()).unwrap()), ["d".into(), "e".into()]);
    assert_eq!(ids(store.every_child_of(&"w".into()).unwrap()), ["a".into(), "d".into(), "e".into()]);
    assert_eq!(store.child_at(&"w".into(), "/c/x").unwrap().map(|c| c.id), Some("d".into()));
}

/// One discarded moves the revision, as a removed record does, so a client following the records hears it's gone.
/// One removed while it was being made stays removed when its worker answers.
#[test]
fn a_record_made_ready_takes_its_branches_and_one_discarded_frees_its_path() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let branches = Branches { head: Head::Branch { name: BranchName::from("alder/x") }, had: vec![] };
    store.insert_child(&child("a", "/c/x"), &"k1".into()).unwrap();
    store.insert_child(&child("b", "/c/y"), &"k2".into()).unwrap();
    store.insert_child(&child("c", "/c/z"), &"k3".into()).unwrap();

    let ready = store.child_ready(&"a".into(), Some(&branches)).unwrap().unwrap();
    store.discard_child(&"b".into()).unwrap();
    let discarded = current_rev(&lock(&store.reader)).unwrap();
    store.discard_child(&"a".into()).unwrap();
    store.child_removed(&"c".into()).unwrap();
    let answered_late = store.child_ready(&"c".into(), None).unwrap();

    assert!(discarded > ready.rev, "the last change before it was {:?}: {discarded:?}", ready.rev);
    assert_eq!((ready.path.as_str(), ready.status, ready.branches), ("/c/x", ChildStatus::Ready, Some(branches)));
    assert!(store.child_at(&"w".into(), "/c/y").unwrap().is_none(), "discarded");
    assert!(store.child_at(&"w".into(), "/c/x").unwrap().is_some(), "only one being made is discarded");
    assert_eq!(answered_late, None);
    let all = store.every_child_of(&"w".into()).unwrap();
    assert!(matches!(all.iter().find(|c| c.id == "c".into()).unwrap().status, ChildStatus::Removed { .. }));
}

#[test]
fn branches_are_recorded_only_when_they_change() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.insert_child(&child("a", "/c/x"), &"k1".into()).unwrap();
    let on = |name: &str| Branches { head: Head::Branch { name: BranchName::from(name) }, had: vec![] };

    let first = store.set_child_branches(&"w".into(), "/c/x", &on("alder/x")).unwrap();
    let same = store.set_child_branches(&"w".into(), "/c/x", &on("alder/x")).unwrap();
    let switched = store.set_child_branches(&"w".into(), "/c/x", &on("alder/y")).unwrap();
    let nowhere = store.set_child_branches(&"w".into(), "/c/z", &on("alder/x")).unwrap();

    let (first, switched) = (first.unwrap(), switched.unwrap());
    assert_eq!(same, None);
    assert!(switched.rev > first.rev);
    assert_eq!(switched.branches, Some(on("alder/y")));
    assert_eq!(nowhere, None, "no worktree there");
}
