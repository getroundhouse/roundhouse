use std::fs;

use protocol::event::{AgentTask, TaskKind};

use super::*;
use crate::store::backup::files_in;
use crate::store::tests::store_with_session;

/// A database as a build with only the first `version` migrations left it, holding a repo. Keep what
/// this returns open, and what it wrote stays in the WAL, as it does when a hub is killed.
fn older_database(path: &Path, backups: &Path, version: usize) -> Connection {
    let mut conn = open_connection(path).unwrap();
    migrate_to(&mut conn, backups, version).unwrap();
    conn.execute("INSERT INTO repos (id, name, path, created_at_ms) VALUES ('r', 'repo', '/repo', 0)", []).unwrap();
    conn
}

#[test]
fn an_older_database_is_copied_with_what_its_wal_holds_before_it_migrates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    let _older = older_database(&path, &backups, 1);

    migrate(&mut open_connection(&path).unwrap(), &backups).unwrap();

    let saved = files_in(&backups);
    assert_eq!(saved.len(), 1, "got {saved:?}");
    let copy = Connection::open(backups.join(&saved[0])).unwrap();
    assert_eq!(schema_version(&copy).unwrap(), 1);
    let repo: String = copy.query_row("SELECT name FROM repos", [], |row| row.get(0)).unwrap();
    assert_eq!(repo, "repo");
    assert_eq!(schema_version(&open_connection(&path).unwrap()).unwrap(), MIGRATIONS.len());
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
    let _current = older_database(&path, &backups, MIGRATIONS.len());

    migrate(&mut open_connection(&path).unwrap(), &backups).unwrap();

    assert_eq!(files_in(&backups), Vec::<String>::new());
}

#[test]
fn a_database_that_cant_be_copied_isnt_migrated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    let older = older_database(&path, &backups, 1);
    // A file where the directory goes, which stops root as well as anyone else.
    fs::write(&backups, "").unwrap();

    let result = migrate(&mut open_connection(&path).unwrap(), &backups);

    assert!(matches!(result, Err(StoreError::Backup { .. })), "got {result:?}");
    assert_eq!(schema_version(&older).unwrap(), 1);
}

/// An update with two migrations, where the second fails, stops part way, and each start after that
/// copies the half-migrated database again. The copy from before the update has to survive that.
#[test]
fn retrying_an_update_that_stops_part_way_keeps_the_copy_from_before_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    let _current = older_database(&path, &backups, MIGRATIONS.len());
    let update = [MIGRATIONS, &["CREATE TABLE fifth (a)", "NOT SQL"]].concat();

    let attempts: Vec<_> =
        (0..4).map(|_| migrate_through(&mut open_connection(&path).unwrap(), &backups, &update)).collect();

    assert!(attempts.iter().all(|attempt| matches!(attempt, Err(StoreError::Database(_)))), "got {attempts:?}");
    let mut versions: Vec<usize> = files_in(&backups)
        .iter()
        .map(|copy| schema_version(&Connection::open(backups.join(copy)).unwrap()).unwrap())
        .collect();
    versions.sort_unstable();
    assert_eq!(versions, [MIGRATIONS.len(), MIGRATIONS.len() + 1]);
}

#[test]
fn a_database_from_a_newer_build_is_refused_and_left_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    let newer = older_database(&path, &backups, MIGRATIONS.len());
    newer.pragma_update(None, "user_version", i64::try_from(MIGRATIONS.len() + 1).unwrap()).unwrap();

    let result = migrate(&mut open_connection(&path).unwrap(), &backups);

    assert!(matches!(result, Err(StoreError::TooNew { .. })), "got {result:?}");
    assert_eq!(schema_version(&newer).unwrap(), MIGRATIONS.len() + 1);
    assert_eq!(files_in(&backups), Vec::<String>::new());
}

/// What's running in the background is on the session's record, so writing it moves the session's revision, as
/// writing the rest of the record does. Every change to it comes with its status today, which moves the revision
/// anyway, so this writes it alone.
#[test]
fn writing_whats_running_in_the_background_gives_the_session_a_newer_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let before = store.get_session(&"s".into()).unwrap().session.rev;
    let task = AgentTask { id: "t".into(), description: Some("Run the tests".into()), kind: TaskKind::Monitor };
    let listed = serde_json::to_string(&[&task]).unwrap();
    store.writer.lock().execute("UPDATE sessions SET background_tasks = ? WHERE id = 's'", [listed]).unwrap();
    let after = store.get_session(&"s".into()).unwrap().session;
    assert!(after.rev > before, "{} isn't past {before}", after.rev);
    assert_eq!(after.background_tasks, [task]);
}

/// Every agent has full access unless it's planning, so a mode only the old picker offered goes, and so does leaving it
/// to the user's own settings, and the chat's next agent has full access.
#[test]
fn a_chat_from_before_full_access_keeps_only_plan_mode() {
    let dir = tempfile::tempdir().unwrap();
    let (path, backups) = (dir.path().join("hub.db"), dir.path().join("backups"));
    let mut conn = older_database(&path, &backups, BEFORE_FULL_ACCESS);
    conn.execute_batch(
        r"
        INSERT INTO workspaces (id, repo_id, name, branch, status, provider, instance_id, created_at_ms)
            VALUES ('w', 'r', 'alder', 'alder', 'ready', 'local', 'w', 0);
        INSERT INTO sessions (id, workspace_id, agent, agent_session_id, status, last_seq, created_at_ms,
                              updated_at_ms, permission_mode)
            VALUES ('asks', 'w', 'claude', 'a', 'idle', 0, 0, 0, 'manual'),
                   ('edits', 'w', 'claude', 'b', 'idle', 0, 0, 0, 'acceptEdits'),
                   ('planning', 'w', 'claude', 'c', 'idle', 0, 0, 0, 'plan'),
                   ('full', 'w', 'claude', 'd', 'idle', 0, 0, 0, 'bypassPermissions'),
                   ('unset', 'w', 'claude', 'e', 'idle', 0, 0, 0, NULL);
        ",
    )
    .unwrap();

    migrate(&mut conn, &backups).unwrap();

    let mode = |id: &str| -> String {
        conn.query_row("SELECT permission_mode FROM sessions WHERE id = ?", [id], |row| row.get(0)).unwrap()
    };
    let modes: Vec<_> = ["asks", "edits", "planning", "full", "unset"].map(mode).into();
    assert_eq!(modes, ["bypassPermissions", "bypassPermissions", "plan", "bypassPermissions", "bypassPermissions"]);
}
