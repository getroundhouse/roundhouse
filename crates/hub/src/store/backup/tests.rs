#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicUsize, Ordering};

use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::*;

/// A database whose WAL holds what a second connection, returned with it, keeps from moving into its file, by reading
/// the state from before it, as another program reading the database can. So a copy of it goes through SQLite.
fn held_back(path: &Path) -> (Connection, Connection) {
    let conn = Connection::open(path).unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.execute_batch("CREATE TABLE notes (text TEXT); INSERT INTO notes VALUES ('read')").unwrap();
    let reading = Connection::open(path).unwrap();
    reading.execute_batch("BEGIN").unwrap();
    reading.query_row("SELECT count(*) FROM notes", [], |_| Ok(())).unwrap();
    conn.execute("INSERT INTO notes VALUES ('written since')", []).unwrap();
    (conn, reading)
}

fn notes(database: &Path) -> Vec<String> {
    let conn = Connection::open(database).unwrap();
    let mut notes = conn.prepare("SELECT text FROM notes ORDER BY rowid").unwrap();
    notes.query_map([], |row| row.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap()
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    #[test]
    fn a_name_reads_back_as_itself(version: usize, number: u64) {
        let name = Name { version, number };

        prop_assert_eq!(Name::parse(&name.to_string()), Some(name));
    }
}

#[test]
fn a_name_it_wouldnt_write_isnt_a_backup() {
    for other in ["notes.txt", UNFINISHED, "hub-v03-1.db", "hub-v3-+1.db", "hub-v3.db", "hub-v3-1.db-wal"] {
        assert_eq!(Name::parse(other), None, "{other}");
    }
}

#[test]
fn each_of_the_newest_few_versions_keeps_its_newest_copy_and_other_files_are_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let conn = Connection::open(dir.path().join("hub.db")).unwrap();
    let backups = dir.path().join("backups");
    fs::create_dir(&backups).unwrap();
    fs::write(backups.join("notes.txt"), "someone's own file").unwrap();

    for version in 1..=KEPT + 1 {
        back_up(&conn, version, &backups).unwrap();
        back_up(&conn, version, &backups).unwrap();
    }

    // Each version's second copy, numbered twice its version, for all but the oldest version.
    let mut expected: Vec<String> =
        (2..=KEPT + 1).map(|version| format!("hub-v{version}-{}.db", 2 * version)).collect();
    expected.push("notes.txt".to_string());
    expected.sort();
    assert_eq!(files_in(&backups), expected);
}

#[test]
fn a_backup_cut_off_part_way_doesnt_stop_the_next() {
    let dir = tempfile::tempdir().unwrap();
    let conn = Connection::open(dir.path().join("hub.db")).unwrap();
    let backups = dir.path().join("backups");
    fs::create_dir(&backups).unwrap();
    fs::write(backups.join(UNFINISHED), "half a database").unwrap();

    back_up(&conn, 1, &backups).unwrap();

    assert_eq!(files_in(&backups), ["hub-v1-1.db"]);
}

/// Once what's in the WAL is in the database's file, the file is the database, so it's what's copied, which a file
/// system that clones files does at once.
#[test]
fn a_database_nothing_else_reads_is_copied_as_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("hub.db");
    let conn = Connection::open(&database).unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.execute_batch("CREATE TABLE notes (text TEXT); INSERT INTO notes VALUES ('kept')").unwrap();

    let path = back_up(&conn, 1, &dir.path().join("backups")).unwrap();

    assert_eq!(fs::read(&path).unwrap(), fs::read(&database).unwrap());
    assert_eq!(notes(&path), ["kept"]);
}

#[test]
fn a_copy_has_what_the_wal_holds_while_another_program_reads_an_older_state() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, _reading) = held_back(&dir.path().join("hub.db"));

    let path = back_up(&conn, 1, &dir.path().join("backups")).unwrap();

    assert_eq!(notes(&path), ["read", "written since"]);
}

/// How many times SQLite asked [`never_wait`] whether to wait for another connection.
static ASKED_TO_WAIT: AtomicUsize = AtomicUsize::new(0);

fn never_wait(_: i32) -> bool {
    ASKED_TO_WAIT.fetch_add(1, Ordering::SeqCst);
    false
}

/// A copy that has to go through SQLite goes at once, where waiting for the reader would hold up a hub's start for its
/// connection's busy timeout.
#[test]
fn a_copy_doesnt_wait_for_whatever_holds_the_wal_back() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, _reading) = held_back(&dir.path().join("hub.db"));
    conn.busy_handler(Some(never_wait)).unwrap();

    back_up(&conn, 1, &dir.path().join("backups")).unwrap();

    assert_eq!(ASKED_TO_WAIT.load(Ordering::SeqCst), 0);
}

#[test]
fn a_database_with_no_file_is_copied_through_sqlite() {
    let dir = tempfile::tempdir().unwrap();
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE notes (text TEXT); INSERT INTO notes VALUES ('in memory')").unwrap();

    let path = back_up(&conn, 1, &dir.path().join("backups")).unwrap();

    assert_eq!(notes(&path), ["in memory"]);
}

#[test]
fn a_copy_that_fails_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, _reading) = held_back(&dir.path().join("hub.db"));
    let backups = dir.path().join("backups");
    // SQLite won't vacuum inside a transaction, so the copy fails once its file exists.
    conn.execute_batch("BEGIN").unwrap();

    let result = back_up(&conn, 1, &backups);

    assert!(matches!(result, Err(BackupError::Vacuum(_))), "got {result:?}");
    assert_eq!(files_in(&backups), Vec::<String>::new());
}

/// Windows has no modes, as `private_files` says.
#[cfg(unix)]
#[test]
fn only_its_owner_can_read_a_backup_however_its_copied() {
    let dir = tempfile::tempdir().unwrap();
    let backups = dir.path().join("backups");
    let database = dir.path().join("hub.db");
    let conn = Connection::open(&database).unwrap();
    conn.execute_batch("CREATE TABLE notes (text TEXT)").unwrap();
    // A copy of a file has its mode.
    fs::set_permissions(&database, fs::Permissions::from_mode(0o644)).unwrap();
    let (held, _reading) = held_back(&dir.path().join("held.db"));

    let copies = [back_up(&conn, 1, &backups).unwrap(), back_up(&held, 2, &backups).unwrap()];

    for path in copies {
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600, "{}", path.display());
    }
    assert_eq!(fs::metadata(&backups).unwrap().permissions().mode() & 0o777, 0o700, "nor list what's kept");
}
