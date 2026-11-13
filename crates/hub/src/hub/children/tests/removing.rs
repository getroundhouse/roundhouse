//! Removing a workspace's other worktrees, for an agent and as the workspace is archived.

use protocol::model::WorkspaceStatus;

use super::*;

/// Removing a worktree that would lose work says what it would lose and changes nothing, unless forced. One removed
/// keeps its record, and asking again answers that it's gone. The workspace's own isn't removed for an agent.
#[tokio::test]
async fn removing_one_that_would_lose_work_answers_what_it_would_lose_unless_forced() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, seen) = workspace(dir.path(), StandIn { others_lose: true, ..StandIn::default() });
    create(&hub, &link, new_branch("try"), "k").await.unwrap();

    let kept = remove(&hub, &link, TRY, false).await.unwrap();
    let removed = remove(&hub, &link, TRY, true).await.unwrap();
    let again = remove(&hub, &link, TRY, false).await.unwrap();
    let own = remove(&hub, &link, "/ws/alder", true).await.unwrap_err();
    let nowhere = remove(&hub, &link, "/elsewhere", true).await.unwrap_err();

    assert!(matches!(kept, WorktreeRemoval::WouldLose(lose) if lose.status.unreferenced_commits));
    let WorktreeRemoval::Removed(WorktreeRemoved { worktree, last: Some(_) }) = removed else { panic!("{removed:?}") };
    assert!(matches!(worktree.status, ChildStatus::Removed { .. }));
    assert!(matches!(again, WorktreeRemoval::Removed(WorktreeRemoved { last: None, .. })), "it's gone");
    assert_eq!((own.code, nowhere.code), (ErrorCode::Conflict, ErrorCode::NotFound));
    assert_eq!(seen.lock().removed, [TRY]);
    assert_eq!(hub.store.every_child_of(&"w".into()).unwrap().len(), 1, "its record stays");
}

/// One whose worktree has gone without the hub knowing is recorded as removed when it's removed, with nothing git
/// could say of it.
#[tokio::test]
async fn removing_one_that_has_gone_records_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, _) = workspace(dir.path(), StandIn { gone: &[TRY], ..StandIn::default() });
    create(&hub, &link, new_branch("try"), "k").await.unwrap();

    let removed = remove(&hub, &link, TRY, false).await.unwrap();

    let WorktreeRemoval::Removed(WorktreeRemoved { worktree, last: None }) = removed else { panic!("{removed:?}") };
    assert!(matches!(worktree.status, ChildStatus::Removed { .. }));
}

/// Archiving refuses while any of the workspace's worktrees would lose work, naming every one, so forcing past one's
/// doesn't lose another's untold. Forced, it removes the others and then the workspace's own, keeping their records.
#[tokio::test]
async fn archiving_names_every_worktree_s_work_it_would_lose_and_forced_removes_them_all() {
    let dir = tempfile::tempdir().unwrap();
    let losing = StandIn { others_lose: true, own_loses: true, ..StandIn::default() };
    let (hub, link, seen) = workspace(dir.path(), losing);
    create(&hub, &link, new_branch("try"), "k").await.unwrap();

    let refused = hub.workspace_archive("w".into(), false).await.unwrap_err();
    let removed_then = seen.lock().removed.clone();
    let archived = hub.workspace_archive("w".into(), true).await;

    assert_eq!(refused.code, ErrorCode::NeedsForce);
    let lines: Vec<_> = refused.message.lines().collect();
    assert!(
        lines.len() == 2 && lines[0].contains("uncommitted") && lines[1].starts_with(&format!("In {TRY}: ")),
        "{lines:?}"
    );
    assert!(removed_then.is_empty(), "refusing changed nothing");
    assert!(archived.is_ok(), "{archived:?}");
    assert_eq!(seen.lock().removed, [TRY, "/ws/alder"]);
    let kept = hub.store.every_child_of(&"w".into()).unwrap();
    assert!(matches!(kept[0].status, ChildStatus::Removed { .. }));
}

/// One an agent wrote in after archiving looked refuses its removal, which says which it is, and leaves the
/// workspace as it was, with its own worktree.
#[tokio::test]
async fn one_that_refuses_its_removal_leaves_the_workspace_s_own_and_the_workspace_ready() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, seen) = workspace(dir.path(), StandIn { refuses_to_remove: Some(TRY), ..StandIn::default() });
    create(&hub, &link, new_branch("try"), "k").await.unwrap();

    let refused = hub.workspace_archive("w".into(), false).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::NeedsForce);
    assert!(refused.message.starts_with(&format!("In {TRY}: ")), "{}", refused.message);
    assert!(seen.lock().removed.is_empty(), "the workspace's own stays");
    assert_eq!(hub.store.get_workspace(&"w".into()).unwrap().status, WorkspaceStatus::Ready);
    assert_eq!(hub.store.children_of(&"w".into()).unwrap().len(), 1, "and it's still recorded as there");
}

/// One that can't be removed at all, as when it's no longer a worktree, refuses as one that would lose work does, and
/// doesn't keep a forced archive from going through, as a failing archive script doesn't. It stays recorded as there,
/// which it is.
#[tokio::test]
async fn a_forced_archive_goes_on_past_one_that_cant_be_removed() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, seen) = workspace(dir.path(), StandIn { cant_remove: Some(TRY), ..StandIn::default() });
    create(&hub, &link, new_branch("try"), "k").await.unwrap();

    let unforced = hub.workspace_archive("w".into(), false).await.unwrap_err();
    let forced = hub.workspace_archive("w".into(), true).await.unwrap();

    assert_eq!(unforced.code, ErrorCode::NeedsForce, "so whoever asked is offered a forced archive");
    assert!(unforced.message.starts_with(&format!("In {TRY}: ")), "{}", unforced.message);
    assert_eq!(forced.status, WorkspaceStatus::Archived);
    assert_eq!(seen.lock().removed, ["/ws/alder"]);
    assert_eq!(hub.store.children_of(&"w".into()).unwrap()[0].status, ChildStatus::Ready);
}
