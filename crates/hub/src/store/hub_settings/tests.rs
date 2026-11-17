use protocol::model::HubSettingsChange;

use crate::store::tests::store_with_session;
use crate::store::{older, schema};

fn keep_awake(on: bool) -> HubSettingsChange {
    HubSettingsChange { keep_awake: Some(on), ..HubSettingsChange::default() }
}

fn allow_asked(on: bool) -> HubSettingsChange {
    HubSettingsChange { allow_asked_in_full_access: Some(on), ..HubSettingsChange::default() }
}

#[test]
fn a_hub_keeps_its_machine_awake_until_someone_chooses_otherwise_and_then_remembers() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let at_first = store.hub_settings().unwrap();
    assert!(at_first.keep_awake);
    assert_eq!(store.list_records().unwrap().hub_settings, Some(at_first.clone()));

    let off = store.change_hub_settings(&keep_awake(false)).unwrap();

    assert!(!off.keep_awake);
    assert!(off.rev > at_first.rev, "{} isn't past {}", off.rev, at_first.rev);
    let listed = store.list_records().unwrap();
    assert_eq!(listed.hub_settings, Some(off.clone()));
    assert_eq!(listed.rev, off.rev, "the list is as new as the change");
    drop(store);
    let (path, backups) = (dir.path().join("hub.db"), dir.path().join("backups"));
    assert_eq!(crate::store::Store::open(&path, &backups).unwrap().hub_settings().unwrap(), off);
}

#[test]
fn setting_the_same_again_or_nothing_says_nothing_new() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let off = store.change_hub_settings(&keep_awake(false)).unwrap();

    assert_eq!(store.change_hub_settings(&keep_awake(false)).unwrap(), off);
    assert_eq!(store.change_hub_settings(&HubSettingsChange::default()).unwrap(), off);
    assert_eq!(store.list_records().unwrap().rev, off.rev);
}

#[test]
fn a_hub_from_before_it_kept_settings_keeps_its_machine_awake() {
    let dir = tempfile::tempdir().unwrap();

    let store = older::store_from_before(dir.path(), schema::BEFORE_HUB_SETTINGS, "");

    let settings = store.hub_settings().unwrap();
    assert!(settings.keep_awake);
    assert!(settings.rev > protocol::model::Rev::default(), "it has a revision, as every record does");
}

#[test]
fn a_hub_asks_the_person_about_a_tool_until_someone_chooses_otherwise_and_then_remembers() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let at_first = store.hub_settings().unwrap();
    assert_eq!(at_first.allow_asked_in_full_access, Some(false));

    let on = store.change_hub_settings(&allow_asked(true)).unwrap();

    assert_eq!(on.allow_asked_in_full_access, Some(true));
    assert!(on.keep_awake, "the other setting is left as it was");
    assert!(on.rev > at_first.rev, "{} isn't past {}", on.rev, at_first.rev);
    assert_eq!(store.change_hub_settings(&allow_asked(true)).unwrap(), on, "the same again says nothing new");
    drop(store);
    let (path, backups) = (dir.path().join("hub.db"), dir.path().join("backups"));
    assert_eq!(crate::store::Store::open(&path, &backups).unwrap().hub_settings().unwrap(), on);
}

#[test]
fn a_hub_from_before_it_could_allow_a_tool_itself_asks_and_keeps_what_it_had() {
    let dir = tempfile::tempdir().unwrap();
    let before = "UPDATE hub_settings SET keep_awake = 0;";

    let store = older::store_from_before(dir.path(), schema::BEFORE_ALLOWING_ASKED, before);

    let settings = store.hub_settings().unwrap();
    assert_eq!(settings.allow_asked_in_full_access, Some(false));
    assert!(!settings.keep_awake);
}

/// A hub from before it recorded what workers are sent has none on its settings until it's recorded, which it is as
/// the hub starts, and the same again moves nothing.
#[test]
fn what_workers_are_sent_is_on_the_settings_once_recorded_and_moves_their_revision_only_as_it_changes() {
    use protocol::model::SentCredential;
    let dir = tempfile::tempdir().unwrap();
    let store = older::store_from_before(dir.path(), schema::BEFORE_GITHUB_SENT, "");
    let before = store.hub_settings().unwrap();
    let account = SentCredential::Account { account: "g".into() };

    let recorded = store.record_github_sent(&SentCredential::MachineOwn).unwrap().unwrap();
    let again = store.record_github_sent(&SentCredential::MachineOwn).unwrap();
    let changed = store.record_github_sent(&account).unwrap().unwrap();

    assert_eq!(before.github.unwrap().sent, None);
    assert_eq!(recorded.github.as_ref().unwrap().sent, Some(SentCredential::MachineOwn));
    assert!(recorded.rev > before.rev);
    assert_eq!(again, None, "the same again moves nothing");
    assert_eq!((changed.github.as_ref().unwrap().sent.as_ref(), changed.rev > recorded.rev), (Some(&account), true));
    assert_eq!(store.list_records().unwrap().hub_settings, Some(changed));
}

#[test]
fn a_hub_is_given_an_id_once_and_keeps_it() {
    let (first, second) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let store = store_with_session(first.path());
    let id = store.hub_id().unwrap();
    assert_eq!(id.as_str().len(), 32, "{id}");

    drop(store);
    let (path, backups) = (first.path().join("hub.db"), first.path().join("backups"));
    assert_eq!(crate::store::Store::open(&path, &backups).unwrap().hub_id().unwrap(), id);
    assert_ne!(store_with_session(second.path()).hub_id().unwrap(), id, "another hub is another");
}

#[test]
fn migrating_chooses_the_newest_usable_login_once_and_keeps_an_expired_choice_visible() {
    use protocol::model::AgentKind;
    let dir = tempfile::tempdir().unwrap();
    let rows = r"
        INSERT INTO accounts (id, login_agent, login_file, login_connected_at_ms, login_ends_at_ms) VALUES
            ('old', 'claude', '00000000-0000-4000-8000-000000000001', 100, NULL),
            ('new', 'claude', '00000000-0000-4000-8000-000000000002', 200, NULL),
            ('expired', 'claude', '00000000-0000-4000-8000-000000000003', 300, 500),
            ('only-expired', 'codex', '00000000-0000-4000-8000-000000000004', 100, 500);
    ";
    let store = older::store_from_before(dir.path(), schema::BEFORE_ACCOUNT_DEFAULTS, rows);
    let defaults = store.hub_settings().unwrap().account_defaults.unwrap();
    assert_eq!(
        defaults.iter().find(|d| d.agent == AgentKind::Claude).map(|d| d.account.as_str()),
        Some("new"),
        "{defaults:?}"
    );
    assert_eq!(defaults.iter().find(|d| d.agent == AgentKind::Codex).unwrap().account.as_str(), "only-expired");
    store.set_account_default(AgentKind::Claude, &"old".into()).unwrap();
    drop(store);
    let store = crate::store::tests::open_in(dir.path());
    let defaults = store.hub_settings().unwrap().account_defaults.unwrap();
    assert_eq!(defaults.iter().find(|d| d.agent == AgentKind::Claude).unwrap().account.as_str(), "old");
}
