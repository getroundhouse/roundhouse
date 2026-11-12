use protocol::attachment::{Attachment, MediaType};
use protocol::model::{AttachmentId, CacheTtl, ContextUsage, Effort, ModelName, PermissionMode, PromptCache};

use super::super::tests::{notice, store_with_session};
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
        permission_mode: Some(PermissionMode::Plan),
        effort: None,
    };
    store.set_session_options(&"s".into(), &others).unwrap();
    assert_eq!(effort(&store), Effort::Medium, "changing the others leaves it");

    store.set_session_options(&"s".into(), &at(Effort::Low)).unwrap();
    assert_eq!(effort(&store), Effort::Low);
}

#[test]
fn a_mode_is_kept_until_another_is_set() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mode = || store.get_session(&"s".into()).unwrap().session.permission_mode;
    assert_eq!(mode(), PermissionMode::FullAccess);

    for chosen in [PermissionMode::Plan, PermissionMode::FullAccess] {
        let change = SessionSettingsChange { permission_mode: Some(chosen), ..SessionSettingsChange::default() };
        store.set_session_options(&"s".into(), &change).unwrap();
        assert_eq!(mode(), chosen);
    }
    store.set_session_options(&"s".into(), &at(Effort::Low)).unwrap();
    assert_eq!(mode(), PermissionMode::FullAccess, "changing the others leaves it");
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

#[test]
fn what_the_cache_holds_is_kept_until_the_hub_says_otherwise_and_moves_the_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.get_session(&"s".into()).unwrap().session;
    assert_eq!(before.prompt_cache, None, "a new session's agent hasn't asked anything");
    let cache = PromptCache { used_at_ms: 5, ttl: CacheTtl::OneHour, model: None, tokens: 252_800 };

    store.set_session_cache(&"s".into(), Some(&cache)).unwrap();

    let after = store.get_session(&"s".into()).unwrap().session;
    assert_eq!(after.prompt_cache, Some(cache));
    assert!(after.rev > before.rev, "two copies of a record at one revision have to be the same");
    store.set_session_cache(&"s".into(), None).unwrap();
    let cleared = store.get_session(&"s".into()).unwrap().session;
    assert_eq!(cleared.prompt_cache, None);
    assert!(cleared.rev > after.rev);
}

#[test]
fn a_session_nothing_has_been_sent_in_is_deleted_with_a_revision_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.get_session(&"s".into()).unwrap().session;

    let (was, rev) = store.delete_unused_session(&"s".into()).unwrap().unwrap();

    assert_eq!(was, before, "it's given back as it was");
    assert!(rev > before.rev, "the feed has to announce it");
    assert!(matches!(store.get_session(&"s".into()), Err(StoreError::NotFound { .. })));
    assert!(matches!(store.delete_unused_session(&"s".into()), Err(StoreError::NotFound { .. })));
}

/// Files are added just before the prompt that carries them, so one no prompt carried is from a prompt that never
/// arrived, and nothing shows it.
#[test]
fn a_file_no_prompt_carried_goes_with_its_session() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let file =
        Attachment { id: AttachmentId::from("a"), name: "a.log".into(), media_type: MediaType::Text, size_bytes: 1 };
    store.add_attachment(&"s".into(), &file).unwrap();

    assert!(store.delete_unused_session(&"s".into()).unwrap().is_some());

    assert_eq!(store.attachment(&"s".into(), &file.id).unwrap(), None);
}

/// Sends something in the session `s`, and checks that it then isn't deleted.
fn kept_once(what: &str, send: impl Fn(&Store)) {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    send(&store);
    let before = store.get_session(&"s".into()).unwrap().session;

    assert_eq!(store.delete_unused_session(&"s".into()).unwrap(), None, "{what}");

    assert_eq!(store.get_session(&"s".into()).unwrap().session, before, "{what} leaves it as it was");
}

#[test]
fn a_session_anything_has_been_sent_in_is_kept() {
    kept_once("an event", |store| {
        store.append(&"s".into(), None, &notice("x"), None, None, None).unwrap();
    });
    kept_once("a queued prompt", |store| {
        let later = QueuedPrompt { client_msg_id: "m".into(), text: "later".into(), attachments: Vec::new() };
        store.queue_prompt(&"s".into(), &later).unwrap();
    });
}

#[test]
fn chats_closed_unused_before_that_deleted_them_go_once_migrated() {
    let dir = tempfile::tempdir().unwrap();
    let rows = r#"
        INSERT INTO sessions (id, workspace_id, agent, model, effort, agent_session_id, status, last_seq,
                              created_at_ms, updated_at_ms, closed_at_ms, queued)
            VALUES ('unused', 'w', 'claude', 'opus', 'xhigh', 'a', 'idle', 0, 0, 0, 1, '[]'),
                   ('used', 'w', 'claude', 'opus', 'xhigh', 'b', 'idle', 1, 0, 0, 1, '[]'),
                   ('queued', 'w', 'claude', 'opus', 'xhigh', 'c', 'idle', 0, 0, 0, 1,
                    '[{"client_msg_id":"m","text":"later"}]'),
                   ('filed', 'w', 'claude', 'opus', 'xhigh', 'd', 'idle', 0, 0, 0, 1, '[]'),
                   ('open', 'w', 'claude', 'opus', 'xhigh', 'e', 'idle', 0, 0, 0, NULL, '[]');
        INSERT INTO events (session_id, seq, ts_ms, kind, body) VALUES ('used', 1, 0, 'notice', '{}');
        INSERT INTO attachments (session_id, id, name, media_type, size_bytes, created_at_ms)
            VALUES ('filed', 'a', 'a.log', 'text', 1, 0);
    "#;

    let store = older::store_from_before(dir.path(), schema::BEFORE_UNUSED_DELETED, rows);

    let kept = |id: &str| store.get_session(&id.into()).is_ok();
    assert!(!kept("unused"));
    assert!(kept("used") && kept("queued") && kept("open"));
    assert!(kept("filed"), "only the hub can let go of a file's bytes");
}

#[test]
fn making_a_chat_is_when_the_person_last_did_something_in_its_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.get_workspace(&"w".into()).unwrap();
    let mut chat = super::super::tests::idle_session(before.created_at_ms + 50);
    chat.session.id = "t".into();

    store.insert_session(&chat, None).unwrap();

    let after = store.get_workspace(&"w".into()).unwrap();
    assert_eq!(after.user_active_at_ms, Some(before.created_at_ms + 50));
    assert!(after.rev > before.rev, "a client only takes a newer copy");
}
