//! The database's schema, and opening it.

use std::path::Path;

use rusqlite::Connection;
use tracing::info;

use super::StoreError;
use super::backup::back_up;
use migrations::MIGRATIONS;

mod migrations;

pub(crate) fn open_connection(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    // With WAL this can lose the last few commits in a power cut, but not corrupt the database.
    // The agent's own session file holds the conversation regardless.
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(conn)
}

/// How many migrations there were before events recorded their run, so a test can make an older database.
#[cfg(test)]
pub(crate) const BEFORE_RUNS: usize = 4;

/// How many migrations there were before workspaces had titles.
#[cfg(test)]
pub(crate) const BEFORE_TITLES: usize = 9;

/// How many migrations there were before sessions had an effort.
#[cfg(test)]
pub(crate) const BEFORE_EFFORT: usize = 13;

/// How many migrations there were before workspaces kept their pull request.
#[cfg(test)]
pub(crate) const BEFORE_PULL_REQUESTS: usize = 14;

/// How many migrations there were before events recorded their subagent.
#[cfg(test)]
pub(crate) const BEFORE_SUBAGENTS: usize = 15;

/// How many migrations there were before subagents had records.
#[cfg(test)]
pub(crate) const BEFORE_SUBAGENT_RECORDS: usize = 16;

/// Brings the database up to this build's schema. One that an older build wrote is copied into
/// `backups_dir` first, since a build from before a migration can't open what it leaves.
pub(crate) fn migrate(conn: &mut Connection, backups_dir: &Path) -> Result<(), StoreError> {
    migrate_through(conn, backups_dir, MIGRATIONS)
}

/// Applies migrations until `target` have run, so a test can make an older database.
#[cfg(test)]
pub(crate) fn migrate_to(conn: &mut Connection, backups_dir: &Path, target: usize) -> Result<(), StoreError> {
    migrate_through(conn, backups_dir, &MIGRATIONS[..target])
}

/// [`migrate`], with the list of migrations given, so a test can stop short of this build's or add one
/// that fails.
fn migrate_through(conn: &mut Connection, backups_dir: &Path, migrations: &[&str]) -> Result<(), StoreError> {
    let version = schema_version(conn)?;
    if version > migrations.len() {
        return Err(StoreError::TooNew { found: version, known: migrations.len() });
    }
    // Only a database that holds something and is about to change gets a copy. If the copy fails, such as
    // on a full disk, the hub doesn't start rather than migrate without one: any update can bring a
    // migration, and nothing would say the way back had gone. The database is untouched at that point, so
    // the build before still opens it.
    if (1..migrations.len()).contains(&version) {
        let backup = back_up(conn, version, backups_dir)
            .map_err(|source| StoreError::Backup { dir: backups_dir.to_path_buf(), source })?;
        info!(backup = %backup.display(), version, "saved a copy of the database before migrating it");
    }
    for (reached, migration) in (1_i64..).zip(migrations).skip(version) {
        let tx = conn.transaction()?;
        tx.execute_batch(migration)?;
        tx.pragma_update(None, "user_version", reached)?;
        tx.commit()?;
    }
    Ok(())
}

/// How many migrations have run on this database.
fn schema_version(conn: &Connection) -> rusqlite::Result<usize> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    // Negative isn't something this code ever writes, so it counts as newer.
    Ok(usize::try_from(version).unwrap_or(usize::MAX))
}

#[cfg(test)]
mod tests;
