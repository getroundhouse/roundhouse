use protocol::model::HubSettingsChange;

use crate::store::tests::store_with_session;
use crate::store::{older, schema};

fn keep_awake(on: bool) -> HubSettingsChange {
    HubSettingsChange { keep_awake: Some(on) }
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
