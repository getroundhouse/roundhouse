use protocol::model::HubSettingsChange;

use crate::store::tests::store_with_session;
use crate::store::{OneValue, lock, older, schema};

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

/// The same again says nothing new either, as `same_again` has it of every record.
#[test]
fn a_change_that_names_no_setting_says_nothing_new() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let off = store.change_hub_settings(&keep_awake(false)).unwrap();

    assert_eq!(store.change_hub_settings(&HubSettingsChange::default()).unwrap(), off);
    assert_eq!(store.list_records().unwrap().rev, off.rev);
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
    drop(store);
    let (path, backups) = (dir.path().join("hub.db"), dir.path().join("backups"));
    assert_eq!(crate::store::Store::open(&path, &backups).unwrap().hub_settings().unwrap(), on);
}

/// A hub from before it recorded the providers whose machines Roundhouse leaves GitHub to has none on its settings, so
/// every client takes every provider as set up, as that hub's were, until the hub that starts records them, the ones
/// left in order of name, and the ones set up in the order given. Recording the same again moves nothing.
#[test]
fn the_providers_left_to_their_machines_and_set_up_are_on_the_settings_once_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let store = older::store_from_before(dir.path(), schema::BEFORE_GITHUB_LEFT_TO_MACHINE, "");
    let before = store.hub_settings().unwrap();

    let (left, set_up) = (["vms".into(), "local".into()], ["keyed".into(), "incus".into()]);
    let recorded = store.record_github_providers(&left, &set_up).unwrap().unwrap();
    let again = store.record_github_providers(&["local".into(), "vms".into()], &set_up).unwrap();

    let github = recorded.github.as_ref().unwrap();
    assert!(before.github.unwrap().sets_up(&"local".into()));
    assert_eq!(github.left_to_machine, ["local".into(), "vms".into()]);
    assert_eq!(github.set_up, ["keyed".into(), "incus".into()]);
    assert!(!github.sets_up(&"local".into()) && github.sets_up(&"incus".into()));
    assert!(recorded.rev > before.rev);
    assert_eq!(again, None);
    assert_eq!(store.list_records().unwrap().hub_settings, Some(recorded));
}

/// A hub from before Settings' Providers kept the `gh` login it found on each provider's machines, which the hub that
/// starts forgets, table and all, and says none of. And it hadn't said which providers GitHub is set up on.
#[test]
fn the_gh_logins_an_older_hub_found_are_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let found = r#"INSERT INTO found_github_logins (provider, login) VALUES ('local', '{"state":"logged_out"}');"#;

    let store = older::store_from_before(dir.path(), schema::BEFORE_GITHUB_SET_UP, found);

    assert_eq!(store.list_records().unwrap().found_github_logins, []);
    assert_eq!(store.hub_settings().unwrap().github.unwrap().set_up, Vec::<protocol::model::ProviderName>::new());
    let tables: i64 = lock(&store.reader)
        .query_row("SELECT COUNT(*) FROM sqlite_master WHERE name = 'found_github_logins'", [], OneValue::read)
        .unwrap();
    assert_eq!(tables, 0);
}

#[test]
fn a_hub_is_given_an_id_once_and_keeps_it() {
    let (first, second) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    // Each runs every migration from nothing, since one of them chooses the ID, and a test's usual store is a copy of
    // one they ran on once.
    let open = |dir: &std::path::Path| crate::store::Store::open(&dir.join("hub.db"), &dir.join("backups")).unwrap();
    let store = open(first.path());
    let id = store.hub_id().unwrap();
    assert_eq!(id.as_str().len(), 32, "{id}");

    drop(store);
    assert_eq!(open(first.path()).hub_id().unwrap(), id);
    assert_ne!(open(second.path()).hub_id().unwrap(), id, "another hub is another");
}
