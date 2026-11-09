use std::fs;
use std::path::PathBuf;

use super::*;

/// A database as a build with only the first `version` migrations left it, holding a repo. Keep what
/// this returns open, and what it wrote stays in the WAL, as it does when a hub is killed.
fn older_database(path: &Path, backups: &Path, version: usize) -> Connection {
    let mut conn = open_connection(path).unwrap();
    migrate_to(&mut conn, backups, version).unwrap();
    conn.execute("INSERT INTO repos (id, name, path, created_at_ms) VALUES ('r', 'repo', '/repo', 0)", []).unwrap();
    conn
}

fn version_of(conn: &Connection) -> usize {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
    usize::try_from(version).unwrap()
}

/// What's in `dir`, which counts as nothing if it isn't there.
fn files_in(dir: &Path) -> Vec<PathBuf> {
    match fs::read_dir(dir) {
        Ok(entries) => entries.map(|entry| entry.unwrap().path()).collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
        Err(e) => panic!("reading {}: {e}", dir.display()),
    }
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
    let copy = Connection::open(&saved[0]).unwrap();
    assert_eq!(version_of(&copy), 1);
    let repo: String = copy.query_row("SELECT name FROM repos", [], |row| row.get(0)).unwrap();
    assert_eq!(repo, "repo");
    assert_eq!(version_of(&open_connection(&path).unwrap()), MIGRATIONS.len());
}

#[test]
fn a_new_database_isnt_copied() {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");

    migrate(&mut open_connection(&dir.path().join("hub.db")).unwrap(), &backups).unwrap();

    assert_eq!(files_in(&backups), Vec::<PathBuf>::new());
}

#[test]
fn an_up_to_date_database_isnt_copied() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let backups = dir.path().join("backups");
    let _current = older_database(&path, &backups, MIGRATIONS.len());

    migrate(&mut open_connection(&path).unwrap(), &backups).unwrap();

    assert_eq!(files_in(&backups), Vec::<PathBuf>::new());
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
    assert_eq!(version_of(&older), 1);
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
    let mut versions: Vec<usize> =
        files_in(&backups).iter().map(|copy| version_of(&Connection::open(copy).unwrap())).collect();
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
    assert_eq!(version_of(&newer), MIGRATIONS.len() + 1);
    assert_eq!(files_in(&backups), Vec::<PathBuf>::new());
}
