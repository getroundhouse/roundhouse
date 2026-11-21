use rusqlite::Connection;
use std::path::Path;

use super::super::{KNOWN, migrate_to, open_connection};

/// A database as a build with only the first `version` migrations left it, holding a repo. Keep what
/// this returns open, and what it wrote stays in the WAL, as it does when a hub is killed. One with every migration
/// starts as a copy of the database they ran on once, as a store's does, so none runs here.
pub(super) fn older_database(path: &Path, backups: &Path, version: usize) -> Connection {
    if version == KNOWN {
        crate::store::migrated::copy_to(path);
    }
    let mut conn = open_connection(path).unwrap();
    migrate_to(&mut conn, backups, version).unwrap();
    conn.execute("INSERT INTO repos (id, name, path, created_at_ms) VALUES ('r', 'repo', '/repo', 0)", []).unwrap();
    conn
}

/// The database at `path` made from `dump`, a database as `sqlite3 .dump` wrote it, opened as the hub opens one, and
/// closed, so what's in its log has been moved into it and the file is whole.
pub(super) fn made_from(path: &Path, dump: &str) {
    let conn = open_connection(path).unwrap();
    conn.execute_batch(dump).unwrap();
    drop(conn);
}
