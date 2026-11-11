use protocol::model::{ContextUsage, Effort, ModelName};

use super::super::tests::store_with_session;
use super::super::{older, schema};
use super::*;

fn effort(store: &Store) -> Effort {
    store.get_session(&"s".into()).unwrap().session.effort
}

/// A change of effort alone.
fn at(effort: Effort) -> SessionSettingsChange {
    SessionSettingsChange { effort: Some(effort), ..SessionSettingsChange::default() }
}

#[test]
fn an_effort_is_kept_until_another_is_set() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    assert_eq!(effort(&store), protocol::model::DEFAULT_EFFORT);

    store.set_session_options(&"s".into(), &at(Effort::Medium)).unwrap();
    assert_eq!(effort(&store), Effort::Medium);

    let others = SessionSettingsChange {
        model: Some(ModelName::named("haiku")),
        permission_mode: Some("plan".into()),
        effort: None,
    };
    store.set_session_options(&"s".into(), &others).unwrap();
    assert_eq!(effort(&store), Effort::Medium, "changing the others leaves it");

    store.set_session_options(&"s".into(), &at(Effort::Low)).unwrap();
    assert_eq!(effort(&store), Effort::Low);
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
fn a_session_from_before_effort_gets_one() {
    let dir = tempfile::tempdir().unwrap();
    let rows = r"
        INSERT INTO sessions (id, workspace_id, agent, model, agent_session_id, status, last_seq, created_at_ms,
                              updated_at_ms)
            VALUES ('s', 'w', 'claude', 'opus', 'a', 'idle', 0, 0, 0);
    ";

    let store = older::store_from_before(dir.path(), schema::BEFORE_EFFORT, rows);

    let session = store.get_session(&"s".into()).unwrap().session;
    assert_eq!((session.model, session.effort), (ModelName::named("opus"), Effort::Xhigh));
    store.set_session_options(&"s".into(), &at(Effort::High)).unwrap();
    assert_eq!(effort(&store), Effort::High);
}

#[test]
fn a_session_that_left_its_model_or_effort_to_the_cli_names_them_once_migrated() {
    let dir = tempfile::tempdir().unwrap();
    let rows = r"
        INSERT INTO sessions (id, workspace_id, agent, model, effort, agent_session_id, status, last_seq,
                              created_at_ms, updated_at_ms)
            VALUES ('none', 'w', 'claude', NULL, NULL, 'a', 'idle', 0, 0, 0),
                   ('default', 'w', 'claude', 'default', 'low', 'b', 'idle', 0, 0, 0),
                   ('blank', 'w', 'claude', '', 'max', 'c', 'idle', 0, 0, 0),
                   ('tab', 'w', 'claude', char(9), 'high', 'e', 'idle', 0, 0, 0),
                   ('chosen', 'w', 'claude', 'haiku', 'medium', 'd', 'idle', 0, 0, 0);
    ";

    let store = older::store_from_before(dir.path(), schema::BEFORE_CHOSEN_MODELS, rows);

    let settings = |id: &str| {
        let session = store.get_session(&id.into()).unwrap().session;
        (session.model, session.effort)
    };
    let opus = ModelName::named("opus");
    assert_eq!(settings("none"), (opus.clone(), Effort::Xhigh));
    assert_eq!(settings("default"), (opus.clone(), Effort::Low), "a level chosen stays");
    assert_eq!(settings("blank"), (opus.clone(), Effort::Max));
    assert_eq!(settings("tab"), (opus, Effort::High));
    assert_eq!(settings("chosen"), (ModelName::named("haiku"), Effort::Medium));
}

#[test]
fn a_level_a_newer_hub_wrote_reads_as_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    lock(&store.writer).execute("UPDATE sessions SET effort = 'ultra' WHERE id = 's'", []).unwrap();

    assert_eq!(effort(&store), Effort::Unknown, "the session still reads");
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
