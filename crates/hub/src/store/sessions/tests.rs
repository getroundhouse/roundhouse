use protocol::attachment::{Attachment, MediaType};
use protocol::capability::settings::Effort;
use protocol::capability::settings::Settings;
use protocol::model::{AttachmentId, ModelName};
use protocol::prompt::Prompt;

use super::super::tests::{notice, store_with_session};
use super::super::{AddedBy, SessionChange};
use super::*;
use protocol::prompt::PromptFrom;

fn effort(store: &Store) -> Effort {
    store.get_session(&"s".into()).unwrap().session.effort
}

/// A change of effort alone.
fn at(effort: Effort) -> Settings {
    Settings { effort: Some(effort), ..Settings::default() }
}

#[test]
fn an_effort_is_kept_until_another_is_set() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    assert_eq!(effort(&store), Effort::Xhigh);

    store.set_session_settings(&"s".into(), &at(Effort::Medium)).unwrap();
    assert_eq!(effort(&store), Effort::Medium);

    let others = Settings {
        model: Some(ModelName::named("haiku")),
        mode: Some(protocol::fixtures::plan_mode()),
        ..Settings::default()
    };
    store.set_session_settings(&"s".into(), &others).unwrap();
    assert_eq!(effort(&store), Effort::Medium, "changing the others leaves it");

    store.set_session_settings(&"s".into(), &at(Effort::Low)).unwrap();
    assert_eq!(effort(&store), Effort::Low);
}

#[test]
fn a_mode_is_kept_until_another_is_set() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let mode = || store.get_session(&"s".into()).unwrap().session.mode;
    assert_eq!(mode(), protocol::fixtures::full_access());

    for chosen in [protocol::fixtures::plan_mode(), protocol::fixtures::full_access()] {
        let change = Settings { mode: Some(chosen), ..Settings::default() };
        store.set_session_settings(&"s".into(), &change).unwrap();
        assert_eq!(mode(), chosen);
    }
    store.set_session_settings(&"s".into(), &at(Effort::Low)).unwrap();
    assert_eq!(mode(), protocol::fixtures::full_access(), "changing the others leaves it");
}

/// The trigger names the column, as typing the modes left it.
#[test]
fn a_new_mode_moves_the_sessions_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.get_session(&"s".into()).unwrap().session.rev;

    let planning = Settings { mode: Some(protocol::fixtures::plan_mode()), ..Settings::default() };
    store.set_session_settings(&"s".into(), &planning).unwrap();

    assert!(store.get_session(&"s".into()).unwrap().session.rev > before, "the feed has to announce it");
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
fn a_level_a_newer_hub_wrote_keeps_its_id() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());

    lock(&store.writer).execute("UPDATE sessions SET effort = 'future' WHERE id = 's'", []).unwrap();

    assert_eq!(effort(&store), Effort::parse("future").unwrap(), "the session retains the chosen ID");
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
    store.add_attachment(&"s".into(), &file, AddedBy::Client).unwrap();

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
        let later = QueuedPrompt { client_msg_id: "m".into(), prompt: Prompt::new("later"), from: PromptFrom::Person };
        store.queue_prompt(&"s".into(), &later).unwrap();
    });
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

/// Kept with what was recorded, so the next hub knows what a worker sends it again.
#[test]
fn what_was_last_recorded_from_a_worker_is_kept_with_the_event_and_left_by_others() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let id = "s".into();
    assert_eq!(store.recorded_from_worker(&id).unwrap(), None);

    let heard = |through| Some(SessionChange { recorded: Some((RunId::new(2), through)), ..SessionChange::default() });
    store.append(&id, None, &notice("a"), None, None, heard(7)).unwrap();
    assert_eq!(store.recorded_from_worker(&id).unwrap(), Some((RunId::new(2), 7)));
    store.append(&id, None, &notice("b"), None, None, Some(SessionStatus::Running.into())).unwrap();
    store.append(&id, None, &notice("c"), None, None, None).unwrap();
    assert_eq!(
        store.recorded_from_worker(&id).unwrap(),
        Some((RunId::new(2), 7)),
        "only what a worker numbered moves it"
    );
    store.append(&id, None, &notice("d"), None, None, heard(9)).unwrap();
    assert_eq!(store.recorded_from_worker(&id).unwrap(), Some((RunId::new(2), 9)));
}

#[test]
fn a_runs_claims_leave_the_record_with_its_exit() {
    use protocol::capability::{Capabilities, Claimed};
    use protocol::event::{EventBody, ExitReason};
    use protocol::model::{RunCapabilities, RunId, SessionId};

    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let id = SessionId::from("s");
    let mut claims = Capabilities::default();
    claims.prompts.steer = Some(Claimed {});
    let claimed = RunCapabilities { run: RunId::new(2), claims: Box::new(claims.clone()) };
    let capabilities = Some(Box::new(claims.clone()));
    let started = EventBody::AgentStarted { pid: 1, resumed: false, build: None, previous_build: None, capabilities };
    let claim = || {
        let change = SessionChange { claims: Some(claimed.clone()), ..SessionChange::default() };
        store.append(&id, Some(RunId::new(2)), &started, None, None, Some(change)).unwrap();
    };
    claim();
    let on_record = || store.get_session(&id).unwrap().session.agent_capabilities;
    let exited =
        EventBody::AgentExited { reason: ExitReason::WorkerLost, code: None, signal: None, stderr_tail: vec![] };
    let exit = |run| store.append(&id, run, &exited, None, None, None).unwrap();

    assert_eq!(on_record(), Some(claimed.clone()));
    exit(Some(RunId::new(1)));
    assert_eq!(on_record(), Some(claimed.clone()), "another run's exit leaves them");
    exit(Some(RunId::new(2)));
    assert_eq!(on_record(), None);

    claim();
    exit(None);
    assert_eq!(on_record(), None, "an exit the hub records with no run is of whichever ran");

    claim();
    store.clear_claims_but(std::slice::from_ref(&id)).unwrap();
    assert_eq!(on_record(), Some(claimed.clone()), "a session whose agent is running keeps them as a hub starts");
    store.clear_claims_but(&[]).unwrap();
    assert_eq!(on_record(), None);

    store.set_claims(&id, &claimed).unwrap();
    assert_eq!(on_record(), Some(claimed), "a run found running has them put back");
}
