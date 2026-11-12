//! Settling the records of a workspace's other worktrees once the hub has its worker, and what the worker reads of
//! their branches.

use protocol::worker::WorkerFrame;

use super::*;

/// What an earlier hub left false is put right from what's there: one being made that was made is recorded as there
/// and watched, one that wasn't is forgotten, so its name is free, and one recorded as there that has gone is
/// recorded as removed. One that's there is watched.
#[tokio::test]
async fn each_record_is_settled_from_whether_its_worktree_is_there() {
    let dir = tempfile::tempdir().unwrap();
    let gone = StandIn { gone: &["/ws/.children/alder/never", "/ws/.children/alder/lost"], ..StandIn::default() };
    let (hub, link, seen) = workspace(dir.path(), gone);
    for (name, key) in [("here", "k1"), ("lost", "k2")] {
        create(&hub, &link, new_branch(name), key).await.unwrap();
    }
    let made = left_being_made(&hub, "made", "k3");
    left_being_made(&hub, "never", "k4");
    taken_so_far(&link).await;
    seen.lock().watched.clear();

    hub.settle_children(&"w".into(), &link).await;

    let status = |work: &str| {
        let all = hub.store.every_child_of(&"w".into()).unwrap();
        all.into_iter().find(|child| child.path == at(work)).map(|child| child.status)
    };
    assert_eq!((status("here"), status("made")), (Some(ChildStatus::Ready), Some(ChildStatus::Ready)));
    assert!(matches!(status("lost"), Some(ChildStatus::Removed { .. })), "{:?}", status("lost"));
    assert_eq!(status("never"), None, "forgotten");
    let existing = ChildBranch::Existing { branch: made.branch };
    assert_eq!(seen.lock().added.last(), Some(&(at("made"), existing)), "asked for as what's there, not made anew");
    taken_so_far(&link).await;
    let mut watched = seen.lock().watched.clone();
    watched.sort();
    assert_eq!(watched, [at("here"), at("made")]);
}

/// Which worktrees the stand-in has been asked to watch since it last had none, once it has been asked for any,
/// which the settling does on a task of its own.
async fn watched(link: &WorkerLink, seen: &Mutex<Seen>) -> Vec<String> {
    let asked = async {
        while seen.lock().watched.is_empty() {
            tokio::task::yield_now().await;
        }
    };
    tokio::time::timeout(Duration::from_secs(20), asked).await.expect("its other worktree was never watched");
    taken_so_far(link).await;
    std::mem::take(&mut seen.lock().watched)
}

/// The records are settled, and those there watched, whenever the hub has a worker watch the workspace's own
/// branches: as it connects to one again or starts one, and once a worktree has been put back.
#[tokio::test]
async fn the_others_are_settled_and_watched_with_the_workspace_s_own() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, seen) = workspace(dir.path(), StandIn::default());
    create(&hub, &link, new_branch("try"), "k").await.unwrap();
    let as_it_was_made = watched(&link, &seen).await;
    let ready = hub.ready_workspace(&"w".into()).await.unwrap();

    hub.watch_branches(&ready, &link).await;
    let as_a_worker_connects = watched(&link, &seen).await;
    hub.watch_branches_from_now(&ready, &link).await;
    let once_put_back = watched(&link, &seen).await;

    assert_eq!(
        (as_it_was_made, as_a_worker_connects, once_put_back),
        (vec![at("try")], vec![at("try")], vec![at("try")])
    );
}

/// What a worker reads of another worktree's branches goes on that worktree's record, only from the worker the
/// workspace has now, since one that's been replaced may still have frames on the way.
#[tokio::test]
async fn another_worktree_s_branches_go_on_its_record_only_from_the_worker_the_workspace_has() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, _) = workspace(dir.path(), StandIn::default());
    create(&hub, &link, new_branch("try"), "k").await.unwrap();
    let replaced = WorkerLink::unconnected();
    let on = |name: &str| Branches { head: Head::Branch { name: BranchName::from(name) }, had: vec![] };
    let frame = |name: &str| WorkerFrame::ChildBranches { path: at("try"), branches: on(name) };
    let recorded = || hub.store.child_at(&"w".into(), &at("try")).unwrap().and_then(|child| child.branches);

    let taken = hub.branches_frame(&"w".into(), &link, frame("alder/try")).await;
    let from_the_current = recorded();
    let from_one_replaced = hub.branches_frame(&"w".into(), &replaced, frame("alder/stale")).await;

    assert!(taken.is_none() && from_one_replaced.is_none(), "each is taken, heard or not");
    assert_eq!(from_the_current, Some(on("alder/try")));
    assert_eq!(recorded(), Some(on("alder/try")), "the replaced worker's wasn't recorded");
}

/// Something else where one was being made, which the worker says when it's asked for it, isn't that worktree, so its
/// record is forgotten. After any other refusal the worktree may be it, so the record stays for the next worker.
#[tokio::test]
async fn one_being_made_is_forgotten_only_when_the_worker_says_something_else_is_there() {
    let dir = tempfile::tempdir().unwrap();
    let refusing = StandIn { taken: &[TRY], cant_make: Some("/ws/.children/alder/failing"), ..StandIn::default() };
    let (hub, link, _) = workspace(dir.path(), refusing);
    left_being_made(&hub, "try", "k1");
    let failing = left_being_made(&hub, "failing", "k2");

    hub.settle_children(&"w".into(), &link).await;

    let left: Vec<_> = hub.store.every_child_of(&"w".into()).unwrap().into_iter().map(|child| child.id).collect();
    assert_eq!(left, [failing.id]);
}

/// A worker that can't say what's there, such as one that went, or one from before the question, changes nothing:
/// taking that for nothing being there would record every worktree as removed.
#[tokio::test]
async fn a_worker_that_cant_say_what_s_there_settles_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, _) = workspace(dir.path(), StandIn::default());
    create(&hub, &link, new_branch("there"), "k1").await.unwrap();
    left_being_made(&hub, "try", "k2");
    let before = hub.store.every_child_of(&"w".into()).unwrap();

    hub.settle_children(&"w".into(), &WorkerLink::unconnected()).await;

    assert_eq!(hub.store.every_child_of(&"w".into()).unwrap(), before);
}
