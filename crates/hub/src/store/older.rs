//! Databases as an older build left them, for the tests of what migrating keeps.

use std::path::Path;

use super::{Store, schema};

/// The store in `dir`, opened on a database that a build with only the first `version` migrations left, holding the
/// repository `r`, its workspace `w`, and `rows`, which are written with that build's columns.
pub(super) fn store_from_before(dir: &Path, version: usize, rows: &str) -> Store {
    let (path, backups) = (dir.join("hub.db"), dir.join("backups"));
    let mut conn = schema::open_connection(&path).unwrap();
    schema::migrate_to(&mut conn, &backups, version).unwrap();
    let repo_and_workspace = "
        INSERT INTO repos (id, name, path, created_at_ms) VALUES ('r', 'repo', '/repo', 0);
        INSERT INTO workspaces (id, repo_id, name, branch, status, provider, instance_id, created_at_ms)
            VALUES ('w', 'r', 'alder', 'alder', 'ready', 'local', 'w', 0);
    ";
    conn.execute_batch(&format!("{repo_and_workspace}{rows}")).unwrap();
    drop(conn);
    Store::open(&path, &backups).unwrap()
}
