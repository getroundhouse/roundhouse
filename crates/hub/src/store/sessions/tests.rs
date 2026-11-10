use protocol::model::{ContextUsage, Effort};

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

#[test]
fn a_session_from_before_context_has_none_and_so_does_one_whose_context_this_hub_cant_read() {
    let dir = tempfile::tempdir().unwrap();
    let rows = r"
        INSERT INTO sessions (id, workspace_id, agent, agent_session_id, status, last_seq, created_at_ms,
                              updated_at_ms)
            VALUES ('s', 'w', 'claude', 'a', 'idle', 0, 0, 0);
    ";

    let store = older::store_from_before(dir.path(), schema::BEFORE_CONTEXT, rows);

    assert_eq!(store.get_session(&"s".into()).unwrap().session.context, None);
    lock(&store.writer).execute("UPDATE sessions SET context = 'not json' WHERE id = 's'", []).unwrap();
    assert_eq!(store.get_session(&"s".into()).unwrap().session.context, None, "the session still reads");
}

fn context(used_tokens: u64) -> ContextUsage {
    ContextUsage { used_tokens, max_tokens: 200_000, parts: Vec::new() }
}

#[test]
fn how_full_the_context_is_is_kept_until_the_agent_says_otherwise_and_moves_the_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.get_session(&"s".into()).unwrap().session;
    assert_eq!(before.context, None, "a new session's agent hasn't said");

    store.set_session_context(&"s".into(), &context(16_000)).unwrap();

    let after = store.get_session(&"s".into()).unwrap().session;
    assert_eq!(after.context, Some(context(16_000)));
    assert!(after.rev > before.rev, "the feed has to announce it");
    store.set_session_options(&"s".into(), &at(Effort::High)).unwrap();
    assert_eq!(
        store.get_session(&"s".into()).unwrap().session.context,
        Some(context(16_000)),
        "another change leaves it"
    );
    store.set_session_context(&"s".into(), &context(17_000)).unwrap();
    assert_eq!(store.get_session(&"s".into()).unwrap().session.context, Some(context(17_000)));
}
