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
