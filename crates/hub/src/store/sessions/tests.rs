use protocol::model::Effort;

use super::super::tests::store_with_session;
use super::super::{older, schema};
use super::*;

fn effort(store: &Store) -> Option<Effort> {
    store.get_session(&"s".into()).unwrap().session.effort
}

/// A change of effort alone.
fn at(effort: Effort) -> SessionOptions {
    SessionOptions { effort: Some(effort), ..SessionOptions::default() }
}

#[test]
fn an_effort_is_kept_until_another_is_set() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    assert_eq!(effort(&store), None, "a new session leaves it to the user's own settings");

    store.set_session_options(&"s".into(), &at(Effort::Xhigh)).unwrap();
    assert_eq!(effort(&store), Some(Effort::Xhigh));

    let others = SessionOptions { model: Some("opus".into()), permission_mode: Some("plan".into()), effort: None };
    store.set_session_options(&"s".into(), &others).unwrap();
    assert_eq!(effort(&store), Some(Effort::Xhigh), "changing the others leaves it");

    store.set_session_options(&"s".into(), &at(Effort::Low)).unwrap();
    assert_eq!(effort(&store), Some(Effort::Low));
}

/// The trigger names the column. Setting options also sets the model, which it names already, so this sets the effort
/// alone.
#[test]
fn a_new_effort_moves_the_sessions_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.get_session(&"s".into()).unwrap().session.rev;

    lock(&store.writer).execute("UPDATE sessions SET effort = 'max' WHERE id = 's'", []).unwrap();

    assert!(store.get_session(&"s".into()).unwrap().session.rev > before, "the feed has to announce it");
}

#[test]
fn a_session_from_before_effort_leaves_it_to_the_users_settings() {
    let dir = tempfile::tempdir().unwrap();
    let rows = r"
        INSERT INTO sessions (id, workspace_id, agent, model, agent_session_id, status, last_seq, created_at_ms,
                              updated_at_ms)
            VALUES ('s', 'w', 'claude', 'opus', 'a', 'idle', 0, 0, 0);
    ";

    let store = older::store_from_before(dir.path(), schema::BEFORE_EFFORT, rows);

    let session = store.get_session(&"s".into()).unwrap().session;
    assert_eq!((session.model.as_deref(), session.effort), (Some("opus"), None));
    store.set_session_options(&"s".into(), &at(Effort::High)).unwrap();
    assert_eq!(effort(&store), Some(Effort::High));
}

#[test]
fn a_level_a_newer_hub_wrote_reads_as_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    lock(&store.writer).execute("UPDATE sessions SET effort = 'ultra' WHERE id = 's'", []).unwrap();

    assert_eq!(effort(&store), Some(Effort::Unknown), "the session still reads");
}
