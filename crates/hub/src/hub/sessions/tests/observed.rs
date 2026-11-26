//! How full a session's agent's context is, and its to-do list, which the hub keeps as observations.

use protocol::capability::output::{Todo, TodoList, TodoStatus};
use protocol::model::{ContextUsage, Observation, Observed, Rev, Session, SessionId};
use rusqlite::Connection;

use crate::hub::tests::{hub_in, hub_in_again};
use crate::hub::{Hub, SessionContext, SessionKey, SessionTodos};
use crate::store::BEFORE_SESSION_CONTEXT_OBSERVED;
use crate::store::columns::OneValue;
use crate::store::older::store_from_before;
use crate::store::tests::idle_session;

/// What `s`'s agent says of its context, with `used_tokens` in it.
fn context(used_tokens: u64) -> ContextUsage {
    ContextUsage { used_tokens, max_tokens: 200_000, model_tokens: None, max_set_by: None, parts: Vec::new() }
}

/// A to-do list of one item.
fn todos() -> TodoList {
    let todo = Todo {
        id: None,
        text: "Write it".into(),
        status: TodoStatus::Pending,
        doing: None,
        priority: None,
        after: Vec::new(),
    };
    TodoList { items: vec![todo], note: None }
}

/// What the hub keeps `s`'s observations by.
fn s() -> SessionKey {
    SessionKey { workspace_id: "w".into(), session_id: "s".into() }
}

/// What `hub` lists at `s`'s level, and its record.
async fn listed(hub: &Hub) -> (Vec<Observation>, Session) {
    let observations = hub.session_records("s".into()).await.unwrap().observations;
    let session = hub.blocking(|store| store.get_session(&"s".into())).await.unwrap().session;
    (observations, session)
}

/// A hub that starts again has both as of the agent's report.
#[tokio::test]
async fn a_sessions_context_and_todos_are_kept_across_a_restart_as_of_the_report() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.observation_read::<SessionContext>(s(), context(60_000), 1_000).await.result().unwrap();
    hub.observation_read::<SessionTodos>(s(), todos(), 2_000).await.result().unwrap();
    hub.observations_written().await;
    drop(hub);

    let again = hub_in_again(dir.path());

    let (observations, _) = listed(&again).await;
    let context_said = Observed::AsOf { value: context(60_000), as_of_ms: 1_000 };
    let todos_said = Observed::AsOf { value: todos(), as_of_ms: 2_000 };
    assert_eq!(
        unrevised(observations),
        [
            Observation::SessionContext { session_id: "s".into(), context: context_said, rev: Rev::default() },
            Observation::SessionTodos { session_id: "s".into(), todos: todos_said, rev: Rev::default() },
        ]
    );
}

/// `observations`, each at no revision, which a test doesn't fix, context first.
fn unrevised(observations: Vec<Observation>) -> Vec<Observation> {
    let mut unrevised: Vec<Observation> = observations
        .into_iter()
        .map(|observation| {
            if let Observation::SessionContext { session_id, context, .. } = observation {
                return Observation::SessionContext { session_id, context, rev: Rev::default() };
            }
            let Observation::SessionTodos { session_id, todos, .. } = observation else { return observation };
            Observation::SessionTodos { session_id, todos, rev: Rev::default() }
        })
        .collect();
    unrevised.sort_by_key(|observation| !matches!(observation, Observation::SessionContext { .. }));
    unrevised
}

/// A database from before both left it opens with the columns gone and the session as it was but for them, and both not
/// known, as nothing is carried out of the database, until the agent next says.
#[tokio::test]
async fn a_session_from_before_opens_with_its_context_and_todos_not_known() {
    let dir = tempfile::tempdir().unwrap();
    let mode = serde_json::to_string(&idle_session(1).session.mode).unwrap();
    let said = serde_json::to_string(&context(60_000)).unwrap();
    let listed_todos = serde_json::to_string(&todos()).unwrap();
    let rows = format!(
        "INSERT INTO sessions (id, workspace_id, agent, model, mode, effort, status, created_at_ms, updated_at_ms,
                 title, context, todos)
             VALUES ('s', 'w', 'claude', 'sonnet', '{mode}', 'high', 'idle', 0, 0, 'Kept', '{said}', '{listed_todos}');"
    );
    drop(store_from_before(dir.path(), BEFORE_SESSION_CONTEXT_OBSERVED, &rows));

    let hub = hub_in_again(dir.path());

    let db = Connection::open(dir.path().join("hub.db")).unwrap();
    let columns: Vec<String> = db
        .prepare("SELECT name FROM pragma_table_info('sessions')")
        .unwrap()
        .query_map([], OneValue::read)
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for gone in ["context", "todos"] {
        assert!(!columns.iter().any(|column| column == gone), "{gone} in {columns:?}");
    }
    let (observations, session) = listed(&hub).await;
    assert_eq!(observations, [], "not known");
    assert_eq!(session.title.as_deref(), Some("Kept"));
}

/// Both go with the session: a hub that starts on a database without it doesn't keep them, and a session closed with
/// nothing sent in it, which is deleted, takes them with it.
#[tokio::test]
async fn a_sessions_context_and_todos_are_forgotten_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    hub.observation_read::<SessionContext>(s(), context(60_000), 1_000).await.result().unwrap();
    hub.observations_written().await;
    drop(hub);
    let db = Connection::open(dir.path().join("hub.db")).unwrap();
    db.execute("DELETE FROM sessions WHERE id = 's'", []).unwrap();

    let again = hub_in_again(dir.path());

    assert!(!again.observations.keeps::<SessionContext>(&s()), "kept though the database lacks the session");
    again.store.insert_session(&idle_session(1), None).unwrap();
    again.observation_read::<SessionTodos>(s(), todos(), 2_000).await.result().unwrap();
    again.session_close(SessionId::from("s")).await.unwrap();
    assert!(!again.observations.keeps::<SessionTodos>(&s()), "kept after the session was deleted");
}
