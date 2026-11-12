use protocol::model::WaitUntil;

use super::*;
use crate::store::tests::{idle_session, store_with_session};

/// A wait of session `s`'s, in workspace `w`, as [`store_with_session`] makes them.
fn wait(id: &str) -> Wait {
    Wait {
        id: id.into(),
        session_id: "s".into(),
        workspace_id: "w".into(),
        number: 7,
        call: Some("toolu_1".into()),
        until: WaitUntil::Checks,
        started_at_ms: 1,
        ended: None,
    }
}

/// The store, with a second chat, `t`, in `w`.
fn store_with_two_chats(dir: &std::path::Path) -> Store {
    let store = store_with_session(dir);
    let mut other = idle_session(1);
    other.session.id = "t".into();
    store.insert_session(&other, None).unwrap();
    store
}

fn how(store: &Store, id: &str) -> Option<WaitEnded> {
    store.wait(&id.into()).unwrap().ended
}

#[test]
fn a_wait_is_kept_as_it_was_asked_for_and_ends_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let asked = wait("a");
    store.start_wait(&asked, Seen::default()).unwrap();

    assert_eq!(store.wait(&"a".into()).unwrap(), asked);
    assert_eq!(store.workspaces_waited_on().unwrap(), HashSet::from(["w".into()]));

    let merged = WaitEnded { at_ms: 7, how: WaitEnd::Merged };
    let ended = store.end_wait(&"a".into(), merged).unwrap();
    assert_eq!(ended, Some(Wait { ended: Some(merged), ..asked }));
    let again = store.end_wait(&"a".into(), WaitEnded { at_ms: 8, how: WaitEnd::Cancelled }).unwrap();
    assert_eq!(again, None, "it ended once");
    assert_eq!(how(&store, "a"), Some(merged));
    assert!(store.workspaces_waited_on().unwrap().is_empty());
}

#[test]
fn a_chat_waits_on_one_at_a_time_so_its_next_ends_its_last() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    let first = store.start_wait(&wait("a"), Seen::default()).unwrap();
    let second = store.start_wait(&Wait { started_at_ms: 5, ..wait("b") }, Seen::default()).unwrap();
    let ended_at_once = WaitEnded { at_ms: 6, how: WaitEnd::Merged };
    let third =
        store.start_wait(&Wait { started_at_ms: 6, ended: Some(ended_at_once), ..wait("c") }, Seen::default()).unwrap();

    assert_eq!((first, second.as_ref().map(WaitId::as_str)), (None, Some("a")));
    assert_eq!(how(&store, "a"), Some(WaitEnded { at_ms: 5, how: WaitEnd::Cancelled }));
    assert_eq!(third.as_ref().map(WaitId::as_str), Some("b"), "even by one that ended as it started");
    assert!(store.workspaces_waited_on().unwrap().is_empty());
}

#[test]
fn a_read_ends_the_workspaces_waits_it_ends_and_leaves_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_two_chats(dir.path());
    store.start_wait(&wait("a"), Seen::default()).unwrap();
    store.start_wait(&Wait { session_id: "t".into(), ..wait("b") }, Seen::default()).unwrap();

    let ended =
        store.judge_waits(&"w".into(), 9, |wait, _| (wait.id.as_str() == "b").then_some(WaitEnd::Merged)).unwrap();
    let again = store.judge_waits(&"w".into(), 10, |_, _| Some(WaitEnd::Closed)).unwrap();

    let merged = WaitEnded { at_ms: 9, how: WaitEnd::Merged };
    assert_eq!(ended, [Wait { session_id: "t".into(), ended: Some(merged), ..wait("b") }]);
    let ids: Vec<_> = again.iter().map(|wait| wait.id.as_str()).collect();
    assert_eq!(ids, ["a"], "one that ended isn't judged again");
    assert_eq!(how(&store, "b"), Some(merged));
}

#[test]
fn closing_a_chat_or_archiving_its_workspace_ends_its_waits() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_two_chats(dir.path());
    store.start_wait(&wait("a"), Seen::default()).unwrap();
    store.start_wait(&Wait { session_id: "t".into(), ..wait("b") }, Seen::default()).unwrap();

    store.set_session_closed(&"s".into(), Some(3)).unwrap();
    let after_closing = (how(&store, "a"), how(&store, "b"));
    let archived = store.set_workspace_archived(&"w".into(), true).unwrap();

    let cancelled = |at_ms| Some(WaitEnded { at_ms, how: WaitEnd::Cancelled });
    assert_eq!(after_closing, (cancelled(3), None), "only the closed chat's");
    assert_eq!(how(&store, "b"), cancelled(archived.archived_at_ms.unwrap()));
    assert!(store.workspaces_waited_on().unwrap().is_empty());
}

/// As its workspace starts being archived, before the archive script runs.
#[test]
fn a_workspaces_waits_are_called_off_together_and_no_others() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_two_chats(dir.path());
    store.start_wait(&wait("a"), Seen::default()).unwrap();
    store.start_wait(&Wait { session_id: "t".into(), ..wait("b") }, Seen::default()).unwrap();

    let elsewhere = store.call_off_workspace_waits(&"v".into(), 2).unwrap();
    let mut called_off = store.call_off_workspace_waits(&"w".into(), 3).unwrap();
    called_off.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));

    assert!(elsewhere.is_empty());
    let ids: Vec<_> = called_off.into_iter().map(|wait| wait.id).collect();
    assert_eq!(ids, [WaitId::from("a"), WaitId::from("b")]);
    let cancelled = Some(WaitEnded { at_ms: 3, how: WaitEnd::Cancelled });
    assert_eq!((how(&store, "a"), how(&store, "b")), (cancelled, cancelled));
}

#[test]
fn a_wait_doesnt_start_in_a_closed_chat_or_an_archived_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_two_chats(dir.path());
    store.set_session_closed(&"s".into(), Some(3)).unwrap();
    let in_a_closed_chat = store.start_wait(&wait("a"), Seen::default());
    store.set_workspace_archived(&"w".into(), true).unwrap();
    let in_an_archived_workspace = store.start_wait(&Wait { session_id: "t".into(), ..wait("b") }, Seen::default());

    assert!(matches!(in_a_closed_chat, Err(StoreError::NotFound { .. })), "{in_a_closed_chat:?}");
    assert!(matches!(in_an_archived_workspace, Err(StoreError::NotFound { .. })), "{in_an_archived_workspace:?}");
    assert!(store.workspaces_waited_on().unwrap().is_empty());
}

#[test]
fn a_wait_nobody_asked_for_isnt_there() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    assert!(matches!(store.wait(&"nothing".into()), Err(StoreError::NotFound { kind: "wait", .. })));
}

#[test]
fn each_wait_is_judged_from_what_was_seen_as_it_started() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_two_chats(dir.path());
    let seen = Seen { left_queue_at_ms: Some(4_000), changes_requested_at_ms: None };
    store.start_wait(&wait("a"), seen).unwrap();
    store.start_wait(&Wait { session_id: "t".into(), ..wait("b") }, Seen::default()).unwrap();

    let mut judged = Vec::new();
    store
        .judge_waits(&"w".into(), 9, |wait, seen| {
            judged.push((wait.id.to_string(), seen));
            None
        })
        .unwrap();

    assert_eq!(judged, [("a".to_string(), seen), ("b".to_string(), Seen::default())]);
}

#[test]
fn a_wait_is_found_by_the_call_that_started_it_whether_or_not_it_has_ended() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.start_wait(&wait("a"), Seen::default()).unwrap();
    store.start_wait(&Wait { call: Some("toolu_2".into()), started_at_ms: 5, ..wait("b") }, Seen::default()).unwrap();

    let first = store.wait_of_call(&"s".into(), &"toolu_1".into()).unwrap();
    let none = store.wait_of_call(&"s".into(), &"toolu_3".into()).unwrap();

    assert_eq!(
        first.map(|wait| (wait.id, wait.ended.map(|ended| ended.how))),
        Some(("a".into(), Some(WaitEnd::Cancelled)))
    );
    assert_eq!(none, None);
}

/// The chat's record shows the wait it has open, and each start and end moves its revision, so the feed announces it,
/// however the wait ends: by the agent, by a read, by the agent's next wait, or as its workspace is archived.
#[test]
fn a_chats_record_shows_its_open_wait_and_moves_as_one_starts_or_ends() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let read = || store.get_session(&"s".into()).unwrap().session;
    let mut revs = vec![read().rev];
    let mut moved = |shown: Option<&str>| {
        let session = read();
        assert_eq!(session.wait.map(|wait| wait.id.to_string()).as_deref(), shown);
        assert!(session.rev > *revs.last().unwrap(), "the revision moved, to show {shown:?}");
        revs.push(session.rev);
    };

    store.start_wait(&wait("a"), Seen::default()).unwrap();
    moved(Some("a"));
    store.end_wait(&"a".into(), WaitEnded { at_ms: 2, how: WaitEnd::Cancelled }).unwrap();
    moved(None);
    store.start_wait(&wait("b"), Seen::default()).unwrap();
    moved(Some("b"));
    store.start_wait(&wait("c"), Seen::default()).unwrap();
    moved(Some("c"));
    store.judge_waits(&"w".into(), 3, |_, _| Some(WaitEnd::Merged)).unwrap();
    moved(None);
    store.start_wait(&wait("d"), Seen::default()).unwrap();
    store.call_off_workspace_waits(&"w".into(), 4).unwrap();
    moved(None);

    assert_eq!(read().wait, None);
    let shown = Wait { id: "e".into(), ..wait("a") };
    store.start_wait(&shown, Seen::default()).unwrap();
    assert_eq!(read().wait, Some(shown), "as it was asked for");
}
