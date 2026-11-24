//! Background tasks stored before the hub kept their kinds.

use protocol::event::{EventBody, TaskKind};
use protocol::model::Seq;

use crate::store::columns::OneValue;
use crate::store::older::store_from_before;
use crate::store::schema::BEFORE_TASK_KINDS_GIVEN;
use crate::store::tests::idle_session;
use crate::store::{Store, lock};

/// A list of tasks as the hub stored one before it kept kinds, and one with kinds.
const NO_KINDS: &str = r#"[{"id":"t1","description":"Run the tests"},{"id":"t2","description":null}]"#;
const KINDS: &str = r#"[{"id":"t3","description":null,"kind":"subagent"}]"#;

/// What `sql` reads of one row and column of `store`'s database.
fn stored(store: &Store, sql: &str) -> String {
    lock(&store.reader).query_row(sql, [], OneValue::read).unwrap()
}

/// Each task stored with no kind is given `command`, in an `agent_tasks` event's list and in a chat's, in its order
/// and beside a task that had a kind. A list whose tasks all have kinds isn't written, so its chat's revision doesn't
/// move, and what isn't JSON, or holds what isn't a task, is left as it was.
#[test]
fn a_task_stored_with_no_kind_is_given_command_once_and_nothing_else_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let mode = serde_json::to_string(&idle_session(1).session.mode).unwrap();
    let chat = |id: &str, tasks: &str| {
        format!(
            "INSERT INTO sessions (id, workspace_id, agent, model, mode, effort, status, created_at_ms, updated_at_ms,
                 background_tasks)
                 VALUES ('{id}', 'w', 'claude', 'sonnet', '{mode}', 'high', 'idle', 0, 0, '{tasks}');"
        )
    };
    let mixed = r#"[{"id":"t1","description":"Run the tests"},{"id":"t2","description":null,"kind":"monitor"}]"#;
    let listed = |seq: u64, tasks: &str| {
        let body = format!(r#"{{"type":"agent_tasks","tasks":{tasks},"stopped":[]}}"#);
        format!(
            "INSERT INTO events (session_id, seq, ts_ms, kind, place, body)
                 VALUES ('s', {seq}, {seq}, 'agent_tasks', {seq}, '{body}');"
        )
    };
    let not_tasks = r#"[{"id":"t4","description":null},["t5"],true]"#;
    let rows = [
        chat("s", mixed),
        chat("t", KINDS),
        chat("u", "not json"),
        chat("v", not_tasks),
        listed(1, NO_KINDS),
        listed(2, KINDS),
        "INSERT INTO events (session_id, seq, ts_ms, kind, place, body) VALUES ('u', 1, 1, 'agent_tasks', 1, 'not json');"
            .to_string(),
    ]
    .concat();
    let store = store_from_before(dir.path(), BEFORE_TASK_KINDS_GIVEN, &rows);

    let kinds =
        |tasks: &[protocol::event::AgentTask]| tasks.iter().map(|task| (task.id.to_string(), task.kind)).collect();
    let events = store.read_events(&"s".into(), Seq::FIRST, 10, u64::MAX, false).unwrap();
    let listed: Vec<Vec<(String, TaskKind)>> = events
        .iter()
        .map(|event| {
            let EventBody::AgentTasks { tasks, .. } = &event.body else { panic!("{event:?}") };
            kinds(tasks)
        })
        .collect();
    let running = |id: &str| kinds(&store.get_session(&id.into()).unwrap().session.background_tasks);
    assert_eq!(listed[0], [("t1".into(), TaskKind::Command), ("t2".into(), TaskKind::Command)]);
    assert_eq!(listed[1], [("t3".into(), TaskKind::Subagent)]);
    assert_eq!(running("s"), [("t1".into(), TaskKind::Command), ("t2".into(), TaskKind::Monitor)]);
    let body = |seq: u64| stored(&store, &format!("SELECT body FROM events WHERE session_id = 's' AND seq = {seq}"));
    assert!(body(1).contains(r#"{"id":"t1","description":"Run the tests","kind":"command"}"#), "{}", body(1));
    assert_eq!(body(2), format!(r#"{{"type":"agent_tasks","tasks":{KINDS},"stopped":[]}}"#), "left as it was");
    let column = |id: &str| stored(&store, &format!("SELECT background_tasks FROM sessions WHERE id = '{id}'"));
    assert_eq!((column("t"), column("u")), (KINDS.to_string(), "not json".to_string()));
    assert_eq!(column("v"), not_tasks, "left as it was, since a list that holds what isn't a task didn't read");
    assert_eq!(stored(&store, "SELECT body FROM events WHERE session_id = 'u'"), "not json");
    let rev = |id: &str| store.get_session(&id.into()).unwrap().session.rev;
    assert!(rev("s") > rev("u") && rev("t") < rev("u"), "only the chat whose list was written moved");
}
