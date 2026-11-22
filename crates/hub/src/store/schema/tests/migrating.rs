use std::fs;

use protocol::event::{AgentTask, TaskKind};

use super::super::*;
use super::older::older_database;
use crate::store::backup::files_in;
use crate::store::tests::store_with_session;
use crate::store::{ByColumn, Columns, OneValue, Store, WorkspacePlacement, columns};

#[test]
fn an_older_database_is_copied_with_what_its_wal_holds_before_it_migrates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    let _older = older_database(&path, &backups, STARTS_AT.migrations);

    migrate(&mut open_connection(&path).unwrap(), &backups).unwrap();

    let saved = files_in(&backups);
    assert_eq!(saved.len(), 1, "got {saved:?}");
    let copy = Connection::open(backups.join(&saved[0])).unwrap();
    assert_eq!(schema_version(&copy).unwrap(), STARTS_AT.migrations);
    let repo: String = copy.query_row("SELECT name FROM repos", [], OneValue::read).unwrap();
    assert_eq!(repo, "repo");
    assert_eq!(schema_version(&open_connection(&path).unwrap()).unwrap(), KNOWN);
}

#[test]
fn a_new_database_isnt_copied() {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");

    migrate(&mut open_connection(&dir.path().join("hub.db")).unwrap(), &backups).unwrap();

    assert_eq!(files_in(&backups), Vec::<String>::new());
}

#[test]
fn an_up_to_date_database_isnt_copied() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    let _current = older_database(&path, &backups, KNOWN);

    migrate(&mut open_connection(&path).unwrap(), &backups).unwrap();

    assert_eq!(files_in(&backups), Vec::<String>::new());
}

#[test]
fn a_database_that_cant_be_copied_isnt_migrated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    let older = older_database(&path, &backups, STARTS_AT.migrations);
    // A file where the directory goes, which stops root as well as anyone else.
    fs::write(&backups, "").unwrap();

    let result = migrate(&mut open_connection(&path).unwrap(), &backups);

    assert!(matches!(result, Err(StoreError::Backup { .. })), "got {result:?}");
    assert_eq!(schema_version(&older).unwrap(), STARTS_AT.migrations);
}

/// An update with two migrations, where the second fails, stops part way, and each start after that
/// copies the half-migrated database again. The copy from before the update has to survive that.
#[test]
fn retrying_an_update_that_stops_part_way_keeps_the_copy_from_before_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    let _current = older_database(&path, &backups, KNOWN);
    let update = [MIGRATIONS, &[Migration::Sql("CREATE TABLE fifth (a)"), Migration::Sql("NOT SQL")]].concat();

    let attempts = Vec::from_fn(4, |_| migrate_through(&mut open_connection(&path).unwrap(), &backups, &update));

    assert!(attempts.iter().all(|attempt| matches!(attempt, Err(StoreError::Database(_)))), "got {attempts:?}");
    let mut versions: Vec<usize> = files_in(&backups)
        .iter()
        .map(|copy| schema_version(&Connection::open(backups.join(copy)).unwrap()).unwrap())
        .collect();
    versions.sort_unstable();
    assert_eq!(versions, [KNOWN, KNOWN + 1]);
}

/// What's running in the background is on the session's record, so writing it moves the session's revision, as
/// writing the rest of the record does. Every change to it comes with its status today, which moves the revision
/// anyway, so this writes it alone.
#[test]
fn writing_whats_running_in_the_background_gives_the_session_a_newer_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.get_session(&"s".into()).unwrap().session.rev;
    let task = AgentTask {
        id: "t".into(),
        description: Some("Run the tests".into()),
        kind: TaskKind::Monitor,
        call: None,
        subagent: None,
    };
    let listed = serde_json::to_string(&[&task]).unwrap();
    store.writer.lock().execute("UPDATE sessions SET background_tasks = ? WHERE id = 's'", [listed]).unwrap();
    let after = store.get_session(&"s".into()).unwrap().session;
    assert!(after.rev > before, "{} isn't past {before}", after.rev);
    assert_eq!(after.background_tasks, [task]);
}

columns! {
    /// The columns [`renaming`] reads of each repo.
    struct NameColumns { id, name }
}

/// Rewrites every repo's name, as a migration in Rust can, where SQL couldn't say how.
fn renaming(tx: &rusqlite::Transaction<'_>) -> Result<(), StoreError> {
    let c = NameColumns::at(0);
    let names: Vec<(String, String)> = tx
        .prepare(&format!("SELECT {} FROM repos", NameColumns::LIST))?
        .query_map([], |row| Ok((row.at(c.id)?, row.at(c.name)?)))?
        .collect::<Result<_, _>>()?;
    for (id, name) in names {
        tx.execute("UPDATE repos SET name = ?1 WHERE id = ?2", (name.to_uppercase(), id))?;
    }
    Ok(())
}

/// Fails once it has changed something, which its transaction takes back.
fn failing(tx: &rusqlite::Transaction<'_>) -> Result<(), StoreError> {
    renaming(tx)?;
    Err(StoreError::NotFound { kind: "a repo", id: "r".into() })
}

/// A migration in Rust runs in its place among the others, and one that fails leaves what it changed undone, and the
/// database at the one before it.
#[test]
fn a_migration_in_rust_runs_in_its_place_and_one_that_fails_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    let _current = older_database(&path, &backups, KNOWN);
    let name = |conn: &Connection| conn.query_row("SELECT name FROM repos", [], OneValue::read::<String>).unwrap();

    let failed = [MIGRATIONS, &[Migration::Rust(failing)]].concat();
    let result = migrate_through(&mut open_connection(&path).unwrap(), &backups, &failed);
    let after_failing =
        (schema_version(&open_connection(&path).unwrap()).unwrap(), name(&open_connection(&path).unwrap()));
    let update = [MIGRATIONS, &[Migration::Rust(renaming), Migration::Sql("CREATE TABLE after (a)")]].concat();
    migrate_through(&mut open_connection(&path).unwrap(), &backups, &update).unwrap();

    assert!(matches!(result, Err(StoreError::NotFound { .. })), "got {result:?}");
    assert_eq!(after_failing, (KNOWN, "repo".to_string()), "nothing it changed is kept");
    let conn = open_connection(&path).unwrap();
    assert_eq!((schema_version(&conn).unwrap(), name(&conn)), (KNOWN + 2, "REPO".to_string()));
}

#[test]
fn workspaces_from_before_placement_migrate_to_their_repos_checkout_without_moving_rev() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    let before_rev: u64 = {
        let conn = older_database(&path, &backups, BEFORE_WORKSPACE_PLACEMENT);
        conn.execute(
            "INSERT INTO workspaces (id, repo_id, name, branch, status, provider, instance_id, created_at_ms)
             VALUES ('w', 'r', 'alder', 'alder/start', 'ready', 'local', 'w', 0)",
            [],
        )
        .unwrap();
        conn.query_row("SELECT rev FROM workspaces WHERE id = 'w'", [], OneValue::read).unwrap()
    };

    let store = Store::open(&path, &backups).unwrap();

    assert_eq!(store.workspace_placement(&"w".into()).unwrap(), WorkspacePlacement::local_checkout("/repo"));
    assert_eq!(
        store.get_workspace(&"w".into()).unwrap().rev,
        protocol::model::Rev::new(before_rev),
        "internal columns don't move rev"
    );
}

#[test]
fn a_github_account_disconnected_before_removal_is_removed_with_its_profile_and_as_the_default_but_an_installation_isnt()
 {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    {
        let conn = older_database(&path, &backups, BEFORE_GITHUB_ACCOUNT_REMOVAL);
        conn.execute_batch(
            "INSERT INTO github_accounts (id, user_id, username) VALUES ('gone', 7, 'ada');
             INSERT INTO github_accounts (id, user_id, username, app_id, installation_id, installed_on_id, installed_on)
                 VALUES ('installed', 9, 'ada-roundhouse[bot]', 1, 2, 7, 'ada');
             INSERT INTO github_accounts (id, user_id, username, login_kind, login_file, login_connected_at_ms)
                 VALUES ('held', 8, 'grace', 'token', 'a-file', 0);
             INSERT INTO github_profiles (account_id, read_at_ms) VALUES ('gone', 0), ('held', 0);
             UPDATE hub_settings SET github_account_id = 'gone';",
        )
        .unwrap();
    }

    let store = Store::open(&path, &backups).unwrap();

    let ids = store.list_records().unwrap().github_accounts.into_iter().map(|account| account.id.to_string());
    assert_eq!(ids.collect::<Vec<_>>(), ["installed", "held"]);
    assert_eq!(store.hub_settings().unwrap().github.and_then(|github| github.account), None);
    let profiles: Vec<String> = {
        let conn = open_connection(&path).unwrap();
        let mut statement = conn.prepare("SELECT account_id FROM github_profiles ORDER BY account_id").unwrap();
        statement.query_map([], OneValue::read).unwrap().collect::<Result<_, _>>().unwrap()
    };
    assert_eq!(profiles, ["held"]);
}
