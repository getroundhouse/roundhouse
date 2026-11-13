//! Nothing is removed as it's made: making one, removing one and archiving take the workspace's gate.

use tokio::sync::mpsc;

use super::*;

/// Archiving waits for one being made before it looks at any, so one made as it starts is checked and removed too.
/// One asked for after finds the workspace archived.
#[tokio::test]
async fn archiving_waits_for_one_being_made_and_removes_it_too() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, seen) = workspace(dir.path(), StandIn::default());
    let making = hub.child_gate(&"w".into()).read_owned().await;
    let archiving = hub.clone();
    let mut archiving = tokio::spawn(async move { archiving.workspace_archive("w".into(), false).await });

    let early = tokio::time::timeout(BOUND, &mut archiving).await;
    let asked_then = seen.lock().calls;
    let made_meanwhile = left_being_made(&hub, "try", "k");
    drop(making);
    let archived = archiving.await.unwrap();
    let late = create(&hub, &link, new_branch("late"), "k2").await.unwrap_err();

    assert!(early.is_err() && asked_then == 0, "it went ahead while one was being made");
    assert!(archived.is_ok(), "{archived:?}");
    assert_eq!(seen.lock().removed, [made_meanwhile.path.as_str(), "/ws/alder"]);
    assert_eq!(late.code, ErrorCode::Conflict);
}

/// One asked for while one is removed, or the workspace archived, waits until that's done.
#[tokio::test]
async fn one_asked_for_waits_while_one_is_removed() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, seen) = workspace(dir.path(), StandIn::default());
    let removing = hub.child_gate(&"w".into()).write_owned().await;
    let (asking, asked) = (hub.clone(), link.clone());
    let mut creating = tokio::spawn(async move { create(&asking, &asked, new_branch("try"), "k").await });

    let early = tokio::time::timeout(BOUND, &mut creating).await;
    let asked_then = seen.lock().added.len();
    drop(removing);

    assert!(early.is_err() && asked_then == 0, "it was made while one was removed");
    assert!(creating.await.unwrap().is_ok());
}

/// One asked for just as archiving starts waits for it, and is refused once it's done, rather than made in a workspace
/// that's on its way out because it looked before the record said so.
#[tokio::test]
async fn one_asked_for_as_archiving_starts_is_refused_rather_than_made() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, seen) = workspace(dir.path(), StandIn::default());
    let archiving = hub.child_gate(&"w".into()).write_owned().await;
    let (asking, asked) = (hub.clone(), link.clone());
    let mut creating = tokio::spawn(async move { create(&asking, &asked, new_branch("try"), "k").await });

    let early = tokio::time::timeout(BOUND, &mut creating).await;
    hub.store.start_move(&"w".into(), Move::Archive).unwrap();
    drop(archiving);
    let refused = creating.await.unwrap().unwrap_err();

    assert!(early.is_err(), "it didn't wait");
    assert_eq!(refused.code, ErrorCode::Conflict);
    assert!(seen.lock().added.is_empty() && hub.store.every_child_of(&"w".into()).unwrap().is_empty());
}

/// A link to a worker that tells `reached` of each request `held` picks and never answers it, and answers the rest as
/// a stand-in does.
fn holding(held: fn(&WorkerRequest) -> bool) -> (Arc<WorkerLink>, mpsc::UnboundedReceiver<()>) {
    let (reaching, reached) = mpsc::unbounded_channel();
    let seen = Mutex::new(Seen::default());
    let link = WorkerLink::answering(move |request| {
        if held(&request) {
            let _ = reaching.send(());
            return None;
        }
        Some(StandIn::default().answer(&seen, request))
    });
    (link, reached)
}

/// Settling holds the gate while it asks the worker what's there, so one being made by an ask isn't forgotten as not
/// there yet, and archiving holds it while the worker removes the worktrees, so none is made beside them.
#[tokio::test]
async fn the_gate_is_held_while_the_records_are_settled_and_while_archiving_removes_the_worktrees() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, _) = workspace(dir.path(), StandIn::default());
    create(&hub, &link, new_branch("there"), "k1").await.unwrap();
    let (asking, mut asked) = holding(|request| matches!(request, WorkerRequest::WorktreeThere(_)));
    let (removing, mut remove_reached) = holding(|request| matches!(request, WorkerRequest::RemoveWorkspace(_)));
    let gate = hub.child_gate(&"w".into());

    let settling = hub.clone();
    let settling = tokio::spawn(async move { settling.settle_children(&"w".into(), &asking).await });
    asked.recv().await.unwrap();
    let free_to_make_while_settling = gate.try_read().is_ok();
    settling.abort();
    let _ = settling.await;

    hub.workers.lock().insert("w".into(), removing);
    let archiving = hub.clone();
    let archiving = tokio::spawn(async move { archiving.workspace_archive("w".into(), true).await });
    remove_reached.recv().await.unwrap();
    let free_to_make_while_archiving = gate.try_read().is_ok();
    archiving.abort();

    assert!(!free_to_make_while_settling);
    assert!(!free_to_make_while_archiving);
}

/// The gate is held for as long as the worker is making one, or removing one, not only while the hub decides to, so
/// neither can start while the worker is still at the other.
#[tokio::test]
async fn the_gate_is_held_while_the_worker_makes_one_and_while_it_removes_one() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, _) = workspace(dir.path(), StandIn::default());
    create(&hub, &link, new_branch("there"), "k1").await.unwrap();
    let (adding, mut add_reached) = holding(|request| matches!(request, WorkerRequest::AddChildWorktree(_)));
    let (removing, mut remove_reached) = holding(|request| matches!(request, WorkerRequest::RemoveWorkspace(_)));
    let gate = hub.child_gate(&"w".into());

    let making = hub.clone();
    let making = tokio::spawn(async move { create(&making, &adding, new_branch("try"), "k2").await });
    add_reached.recv().await.unwrap();
    let free_to_remove_while_one_is_made = gate.try_write().is_ok();
    making.abort();
    let _ = making.await;

    let taking = hub.clone();
    let taking = tokio::spawn(async move { remove(&taking, &removing, &at("there"), true).await });
    remove_reached.recv().await.unwrap();
    let free_to_make_while_one_is_removed = gate.try_read().is_ok();
    taking.abort();

    assert!(!free_to_remove_while_one_is_made);
    assert!(!free_to_make_while_one_is_removed);
}
