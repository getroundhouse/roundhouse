use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

use super::{CacheFile, Part, drop_named};
use crate::store::columns::OneValue;

/// How long these tests' files wait for another connection that holds them, which the holder outlasts, since it lets
/// go only once the file has opened.
const BUSY_TIMEOUT: Duration = Duration::from_millis(50);

/// A part made up for these tests, which keeps its rows in a table named for it, and makes the table where it isn't
/// there.
static ONE: Part = Part { name: "one", prepare: |conn| made(conn, "one"), empty: |conn| drop_named(conn, ["one"]) };

/// Another part like [`ONE`].
static TWO: Part = Part { name: "two", prepare: |conn| made(conn, "two"), empty: |conn| drop_named(conn, ["two"]) };

/// A part whose tables can't be made, even once emptied.
static NEVER: Part =
    Part { name: "never", prepare: |conn| conn.execute_batch("SELECT x FROM nowhere"), empty: |_| Ok(()) };

/// A part that can't write its tables, as on a full disk.
static FULL: Part = Part {
    name: "full",
    prepare: |_| Err(rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_FULL), None)),
    empty: |_| Ok(()),
};

/// Makes `table`, failing where it's of another form, which hasn't the column `x`.
fn made(conn: &mut Connection, table: &str) -> rusqlite::Result<()> {
    conn.execute_batch(&format!("CREATE TABLE IF NOT EXISTS {table} (x INTEGER NOT NULL)"))?;
    conn.prepare(&format!("SELECT x FROM {table}"))?;
    Ok(())
}

/// Makes the file at `path`, with [`ONE`] and [`TWO`], and keeps a row in each.
fn kept_in_both(path: &Path) {
    let file = CacheFile::open_waiting(path, &[&ONE, &TWO], BUSY_TIMEOUT);
    for part in [&ONE, &TWO] {
        let kept = file.with(part, |conn| conn.execute(&format!("INSERT INTO {} (x) VALUES (1)", part.name), []));
        assert_eq!(kept.unwrap(), Some(1));
    }
}

/// How many rows `file` keeps of `part`, or `None` where it neither reads nor writes it.
fn rows(file: &CacheFile, part: &Part) -> Option<i64> {
    file.with(part, |conn| conn.query_row(&format!("SELECT count(*) FROM {}", part.name), [], OneValue::read)).unwrap()
}

/// A file another connection holds as the hub opens it, as the hub before this one can as it stops, is left as it is:
/// a part whose tables it would have had to make is neither read nor written, the others are read as they are, and
/// once it's let go, a hub that starts again has what the file kept, each part made as it opens.
#[test]
fn a_file_another_holds_as_it_opens_keeps_what_it_held() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    kept_in_both(&path);
    let holder = Connection::open(&path).unwrap();
    holder.execute_batch("DROP TABLE two").unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();

    let held = CacheFile::open_waiting(&path, &[&ONE, &TWO], BUSY_TIMEOUT);

    assert_eq!((rows(&held, &ONE), rows(&held, &TWO)), (Some(1), None), "two, which had to be made, unread");
    drop(held);
    holder.execute_batch("ROLLBACK; PRAGMA locking_mode = EXCLUSIVE; BEGIN IMMEDIATE").unwrap();

    let held_whole = CacheFile::open_waiting(&path, &[&ONE, &TWO], BUSY_TIMEOUT);

    assert_eq!((rows(&held_whole, &ONE), rows(&held_whole, &TWO)), (None, None), "held from the start");
    drop((held_whole, holder));
    let let_go = CacheFile::open_waiting(&path, &[&ONE, &TWO], BUSY_TIMEOUT);
    assert_eq!((rows(&let_go, &ONE), rows(&let_go, &TWO)), (Some(1), Some(0)), "kept, and two made");
}

/// A part whose tables can't be made, as where a table of its has another form, is emptied and made again, and one
/// that can't be made even then is neither read nor written, each leaving the other parts as they were.
#[test]
fn a_part_whose_tables_can_t_be_made_is_emptied_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    kept_in_both(&path);
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("DROP TABLE two; CREATE TABLE two (y); INSERT INTO two (y) VALUES (1)").unwrap();

    let file = CacheFile::open_waiting(&path, &[&ONE, &TWO, &NEVER], BUSY_TIMEOUT);

    assert_eq!(rows(&file, &ONE), Some(1), "one kept");
    assert_eq!(rows(&file, &TWO), Some(0), "two emptied and made again");
    assert_eq!(rows(&file, &NEVER), None);
}

/// Only a file SQLite says isn't a database, or is damaged, is made again. One the hub can't open for another reason,
/// which says nothing of what's in it, as one it can't write, is left as it is, neither read nor written, so a hub that
/// starts again once it can has what it kept.
#[test]
fn only_a_file_sqlite_can_t_read_is_made_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    kept_in_both(&path);
    let writable = std::fs::metadata(&path).unwrap().permissions();
    let mut read_only = writable.clone();
    read_only.set_readonly(true);
    std::fs::set_permissions(&path, read_only).unwrap();
    let can_write = std::fs::OpenOptions::new().append(true).open(&path).is_ok();
    assert!(!can_write, "a file made read-only can be written, as by root, so the test can't make one the hub can't");

    let unwritable = CacheFile::open_waiting(&path, &[&ONE, &TWO], BUSY_TIMEOUT);

    assert_eq!((rows(&unwritable, &ONE), rows(&unwritable, &TWO)), (None, None), "neither read nor written");
    drop(unwritable);
    std::fs::set_permissions(&path, writable).unwrap();
    let writable_again = CacheFile::open_waiting(&path, &[&ONE, &TWO], BUSY_TIMEOUT);
    assert_eq!((rows(&writable_again, &ONE), rows(&writable_again, &TWO)), (Some(1), Some(1)), "kept");
    drop(writable_again);

    let full = CacheFile::open_waiting(&path, &[&ONE, &FULL, &TWO], BUSY_TIMEOUT);
    assert_eq!((rows(&full, &ONE), rows(&full, &TWO)), (None, None), "a part can't be written, so neither is read");
    drop(full);
    let kept = CacheFile::open_waiting(&path, &[&ONE, &TWO], BUSY_TIMEOUT);
    assert_eq!((rows(&kept, &ONE), rows(&kept, &TWO)), (Some(1), Some(1)), "kept on a full disk");
    drop(kept);
    std::fs::write(&path, "not a database").unwrap();

    let not_a_database = CacheFile::open_waiting(&path, &[&ONE, &TWO], BUSY_TIMEOUT);

    assert_eq!((rows(&not_a_database, &ONE), rows(&not_a_database, &TWO)), (Some(0), Some(0)), "made again");
}
