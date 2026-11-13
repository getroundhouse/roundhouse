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
