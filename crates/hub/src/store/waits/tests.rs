use protocol::model::WaitUntil;

use super::*;
use crate::store::columns::OneValue;
use crate::store::tests::{idle_session, store_with_session};
use crate::store::{Ended, Move};

/// Archives the workspace `w`, as the hub does once its worktree has gone.
fn archive(store: &Store) -> protocol::model::Workspace {
    store.start_move(&"w".into(), Move::Archive).unwrap().expect("it was ready");
    store.end_move(&"w".into(), Move::Archive, Ended::Done).unwrap()
}

/// A wait of session `s`'s, in workspace `w`, as [`store_with_session`] makes them.
fn wait(id: &str) -> Wait {
    Wait {
        id: id.into(),
        session_id: "s".into(),
        workspace_id: "w".into(),
        number: 7,
        call: Some(format!("toolu_{id}").into()),
        until: WaitUntil::Checks,
        watch: None,
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
    store.start_wait(&asked).unwrap();

    assert_eq!(store.wait(&"a".into()).unwrap(), asked);
    assert_eq!(store.workspaces_waited_on().unwrap(), HashSet::from(["w".into()]));

    let merged = WaitEnded { at_ms: 7, how: WaitEnd::Merged };
    let ended = store.end_wait(&"a".into(), merged).unwrap();
    assert_eq!(ended, Some(Wait { ended: Some(merged), ..asked }));
    let again = store.end_wait(&"a".into(), WaitEnded { at_ms: 8, how: WaitEnd::Cancelled }).unwrap();
    assert_eq!(again, None, "it ended once");
    assert_eq!(how(&store, "a"), Some(merged));
    assert!(store.workspaces_waited_on().unwrap().is_empty());
    store.start_wait(&protocol::fixtures::pull_request_wait("b")).unwrap();
    assert_eq!(store.workspaces_waited_on().unwrap(), HashSet::from(["w".into()]), "one on explicit pull requests too");
}

#[test]
fn independent_waits_coexist_and_retries_reuse_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let first = store.start_wait(&wait("a")).unwrap();
    let second = store.start_wait(&Wait { started_at_ms: 5, ..wait("b") }).unwrap();
    let retried = store.start_wait(&Wait { id: "retry".into(), ..first.clone() }).unwrap();

    assert_eq!(retried, first);
    let session = store.get_session(&"s".into()).unwrap().session;
    assert_eq!(session.open_waits(), [first.clone(), second]);
    assert_eq!(session.wait, Some(first));
    assert_eq!(how(&store, "a"), None);
    assert_eq!(how(&store, "b"), None);
}

#[test]
fn closing_a_chat_or_archiving_its_workspace_ends_its_waits() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_two_chats(dir.path());
    store.start_wait(&wait("a")).unwrap();
    store.start_wait(&Wait { session_id: "t".into(), ..wait("b") }).unwrap();

    store.set_session_closed(&"s".into(), Some(3)).unwrap();
    let after_closing = (how(&store, "a"), how(&store, "b"));
    let archived = archive(&store);

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
    store.start_wait(&wait("a")).unwrap();
    store.start_wait(&Wait { session_id: "t".into(), ..wait("b") }).unwrap();

    let elsewhere = store.call_off_workspace_waits(&"v".into(), 2).unwrap();
    let mut called_off = store.call_off_workspace_waits(&"w".into(), 3).unwrap();
    called_off.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));

    assert!(elsewhere.is_empty());
    let ids: Vec<_> = called_off.into_iter().map(|wait| wait.id).collect();
    assert_eq!(ids, [WaitId::from("a"), WaitId::from("b")]);
    let cancelled = Some(WaitEnded { at_ms: 3, how: WaitEnd::Cancelled });
    assert_eq!((how(&store, "a"), how(&store, "b")), (cancelled, cancelled));
}

/// Only the wait named, and only while the chat has it open: not one that has ended, nor another chat's, and sent again
/// after the agent has started another, not that one.
#[test]
fn a_chats_open_wait_is_called_off_by_its_id_and_no_other() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_two_chats(dir.path());
    store.start_wait(&wait("a")).unwrap();
    store.end_wait(&"a".into(), WaitEnded { at_ms: 2, how: WaitEnd::Merged }).unwrap();
    store.start_wait(&Wait { call: Some("toolu_2".into()), ..wait("b") }).unwrap();
    store.start_wait(&Wait { session_id: "t".into(), ..wait("c") }).unwrap();
    let call_off = |wait: &str, at_ms| store.call_off_session_wait(&"s".into(), &wait.into(), at_ms).unwrap();

    let (ended, elsewhere, called_off) = (call_off("a", 3), call_off("c", 3), call_off("b", 3));
    store.start_wait(&Wait { call: Some("toolu_3".into()), ..wait("d") }).unwrap();
    let again = call_off("b", 4);

    assert_eq!([ended, elsewhere, called_off, again], [None, None, Some(WaitId::from("b")), None]);
    let ended = |at_ms, how| Some(WaitEnded { at_ms, how });
    assert_eq!(how(&store, "a"), ended(2, WaitEnd::Merged), "an ended one stays as it ended");
    assert_eq!(how(&store, "b"), ended(3, WaitEnd::CalledOffByPerson));
    assert_eq!((how(&store, "c"), how(&store, "d")), (None, None), "another chat's, and the agent's next");
}

#[test]
fn a_wait_doesnt_start_in_a_closed_chat_or_an_archived_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_two_chats(dir.path());
    store.set_session_closed(&"s".into(), Some(3)).unwrap();
    let in_a_closed_chat = store.start_wait(&wait("a"));
    store.start_move(&"w".into(), Move::Archive).unwrap();
    let as_it_is_archived = store.start_wait(&Wait { session_id: "t".into(), ..wait("b") });
    store.end_move(&"w".into(), Move::Archive, Ended::Done).unwrap();
    let in_an_archived_workspace = store.start_wait(&Wait { session_id: "t".into(), ..wait("b") });

    assert!(matches!(in_a_closed_chat, Err(StoreError::NotFound { .. })), "{in_a_closed_chat:?}");
    assert!(matches!(as_it_is_archived, Err(StoreError::NotFound { .. })), "{as_it_is_archived:?}");
    assert!(matches!(in_an_archived_workspace, Err(StoreError::NotFound { .. })), "{in_an_archived_workspace:?}");
    assert!(store.workspaces_waited_on().unwrap().is_empty());
}

#[test]
fn a_wait_nobody_asked_for_isnt_there() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    assert!(matches!(store.wait(&"nothing".into()), Err(StoreError::NotFound { kind: "wait", .. })));
}

/// `a` is a wait an older worker's tool started on the workspace's own pull request, which nothing judges now, so it's
/// called off, where `b`, on explicit pull requests, goes on, and `c`, which had ended, keeps how it ended.
#[test]
fn only_open_waits_on_the_workspaces_own_pull_request_are_called_off() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_two_chats(dir.path());
    store.start_wait(&wait("a")).unwrap();
    store.start_wait(&protocol::fixtures::pull_request_wait("b")).unwrap();
    store.start_wait(&Wait { session_id: "t".into(), ..wait("c") }).unwrap();
    let merged = WaitEnded { at_ms: 7, how: WaitEnd::Merged };
    store.end_wait(&"c".into(), merged).unwrap();

    let called_off = store.call_off_unwatched_waits(9).unwrap();

    assert_eq!(called_off.iter().map(|wait| wait.id.as_str()).collect::<Vec<_>>(), ["a"]);
    assert_eq!(how(&store, "a"), Some(WaitEnded { at_ms: 9, how: WaitEnd::Cancelled }));
    assert_eq!((how(&store, "b"), how(&store, "c")), (None, Some(merged)));
}

#[test]
fn a_wait_is_found_by_the_call_that_started_it_whether_or_not_it_has_ended() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.start_wait(&wait("a")).unwrap();
    store.start_wait(&Wait { call: Some("toolu_2".into()), started_at_ms: 5, ..wait("b") }).unwrap();

    let first = store.wait_of_call(&"s".into(), &"toolu_a".into()).unwrap();
    let none = store.wait_of_call(&"s".into(), &"toolu_3".into()).unwrap();

    assert_eq!(first.map(|wait| (wait.id, wait.ended.map(|ended| ended.how))), Some(("a".into(), None)));
    assert_eq!(none, None);
}

/// The chat's record shows the wait it has open, and each start and end moves its revision, so the feed announces it,
/// however the wait ends: by the agent, as the hub calls off those of an older worker's tool, or as its workspace is
/// archived.
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

    store.start_wait(&wait("a")).unwrap();
    moved(Some("a"));
    store.end_wait(&"a".into(), WaitEnded { at_ms: 2, how: WaitEnd::Cancelled }).unwrap();
    moved(None);
    store.start_wait(&wait("b")).unwrap();
    moved(Some("b"));
    store.start_wait(&wait("c")).unwrap();
    moved(Some("b"));
    store.call_off_unwatched_waits(3).unwrap();
    moved(None);
    store.start_wait(&wait("d")).unwrap();
    store.call_off_workspace_waits(&"w".into(), 4).unwrap();
    moved(None);

    assert_eq!(read().wait, None);
    let shown = wait("e");
    store.start_wait(&shown).unwrap();
    assert_eq!(read().wait, Some(shown), "as it was asked for");
}

/// A wait an older hub kept keeps what it asked for, what it saw as it started and its deadline, but none of the pull
/// requests it saw, which were kept in a form that isn't their facets, so the hub's next read finds them again. One
/// that's open moves its chat's revision, so a client hears it. A watch that isn't one is left as it was.
#[test]
fn a_wait_from_before_facets_keeps_its_watch_but_the_pull_requests_it_saw() {
    let dir = tempfile::tempdir().unwrap();
    let mut watch = serde_json::to_value(protocol::fixtures::pull_request_wait("a").watch.unwrap()).unwrap();
    watch["snapshot"]["pull_requests"] = serde_json::json!([{"pull_request": {"number": 7, "url": "u"}, "head": "h"}]);
    let mode = serde_json::to_string(&idle_session(1).session.mode).unwrap();
    let rows = format!(
        "INSERT INTO sessions (id, workspace_id, agent, model, mode, effort, status, created_at_ms, updated_at_ms)
             VALUES ('s', 'w', 'claude', 'sonnet', '{mode}', 'high', 'idle', 0, 0),
                 ('t', 'w', 'claude', 'sonnet', '{mode}', 'high', 'idle', 0, 0),
                 ('u', 'w', 'claude', 'sonnet', '{mode}', 'high', 'idle', 0, 0);
         INSERT INTO waits (id, session_id, workspace_id, number, call, until, started_at_ms, watch)
             VALUES ('a', 's', 'w', 7, NULL, 'checks', 1, '{watch}'),
                 ('b', 'u', 'w', 8, NULL, 'checks', 1, 'not a watch');"
    );

    let store = crate::store::older::store_from_before(dir.path(), crate::store::schema::BEFORE_WAITS_AS_FACETS, &rows);

    watch["snapshot"]["pull_requests"] = serde_json::json!([]);
    let expected: protocol::model::PullRequestWatch = serde_json::from_value(watch).unwrap();
    assert_eq!(store.wait(&"a".into()).unwrap().watch, Some(Box::new(expected)));
    let rev = |id: &str| store.get_session(&id.into()).unwrap().session.rev;
    assert!(rev("s") > rev("t"), "its chat's revision moved");
    let conn = rusqlite::Connection::open(dir.path().join("hub.db")).unwrap();
    let left: Option<String> = conn.query_row("SELECT watch FROM waits WHERE id = 'b'", [], OneValue::read).unwrap();
    assert_eq!(left.as_deref(), Some("not a watch"));
}
