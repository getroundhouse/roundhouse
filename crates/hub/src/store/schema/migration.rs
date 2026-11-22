use super::super::StoreError;
use super::super::backup::back_up;
use super::migrations;
use migrations::{AT_FLOOR, MIGRATIONS};
use rusqlite::Connection;
use std::path::Path;
use tracing::info;
use update_feed::{FLOOR, Floor};

/// One step from one schema to the next: SQL, or Rust for one that SQL can't say well, such as rewriting what's
/// stored from one form to another. Each runs in a transaction of its own, which commits only once it's done, so one
/// that fails part way leaves the database as the step before left it.
#[derive(Clone, Copy)]
pub(super) enum Migration {
    Sql(&'static str),
    Rust(fn(&rusqlite::Transaction<'_>) -> Result<(), StoreError>),
}

pub(crate) fn open_connection(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    // With WAL this can lose the last few commits in a power cut, but not corrupt the database.
    // The agent's own session file holds the conversation regardless.
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    // A checkpoint copies the log into the database and starts the log again, and on a Mac only a full sync keeps
    // those writes in order on the drive. Without one, a power cut can take back a commit that had reached the disk,
    // such as one `Store::lasting` made, or corrupt the database. Every connection, since whichever closes last
    // checkpoints. It costs one full sync for each checkpoint, not each commit.
    conn.pragma_update(None, "checkpoint_fullfsync", true)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(conn)
}

/// The floor this build's schema starts at, the database [`AT_FLOOR`] makes. A build that updates from any release
/// would need every migration from the first, which this one doesn't have, so it doesn't build without a floor.
pub(super) const STARTS_AT: Floor = match FLOOR {
    Some(floor) => floor,
    None => panic!("the hub's schema starts at its floor"),
};

/// How many migrations a database this build migrates has run once it's done, counting the floor's.
#[cfg(test)]
pub(super) const KNOWN: usize = STARTS_AT.migrations + MIGRATIONS.len();

/// Brings the database up to this build's schema. One that an older build wrote is copied into
/// `backups_dir` first, since a build from before a migration can't open what it leaves.
pub(crate) fn migrate(conn: &mut Connection, backups_dir: &Path) -> Result<(), StoreError> {
    migrate_through(conn, backups_dir, MIGRATIONS)
}

/// Applies migrations until `target` have run, counting the floor's, so a test can make an older database. There's
/// none older than the floor's to make.
#[cfg(test)]
pub(crate) fn migrate_to(conn: &mut Connection, backups_dir: &Path, target: usize) -> Result<(), StoreError> {
    let since = target.checked_sub(STARTS_AT.migrations).expect("a database at the floor or newer");
    migrate_through(conn, backups_dir, &MIGRATIONS[..since])
}

/// [`migrate`], with the migrations since the floor given, so a test can stop short of this build's, or add one that
/// fails. A new database starts from the floor's schema, and counts the floor's migrations as run.
pub(super) fn migrate_through(
    conn: &mut Connection,
    backups_dir: &Path,
    since: &[Migration],
) -> Result<(), StoreError> {
    let Floor { release, migrations: at_floor } = STARTS_AT;
    let known = at_floor + since.len();
    let version = schema_version(conn)?;
    if version > known {
        return Err(StoreError::TooNew { found: version, known });
    }
    // A new database holds nothing, so it isn't older than the floor.
    if (1..at_floor).contains(&version) {
        return Err(StoreError::TooOld { found: version, release, migrations: at_floor });
    }
    // Only a database that holds something and is about to change gets a copy. If the copy fails, such as
    // on a full disk, the hub doesn't start rather than migrate without one: any update can bring a
    // migration, and nothing would say the way back had gone. The database is untouched at that point, so
    // the build before still opens it.
    if (1..known).contains(&version) {
        let backup = back_up(conn, version, backups_dir)
            .map_err(|source| StoreError::Backup { dir: backups_dir.to_path_buf(), source })?;
        info!(backup = %backup.display(), version, "saved a copy of the database before migrating it");
    }
    if version == 0 {
        let tx = conn.transaction()?;
        tx.execute_batch(AT_FLOOR)?;
        tx.pragma_update(None, "user_version", at_floor)?;
        tx.commit()?;
    }
    for (reached, migration) in (at_floor + 1..).zip(since).skip(version.saturating_sub(at_floor)) {
        let tx = conn.transaction()?;
        match migration {
            Migration::Sql(sql) => tx.execute_batch(sql)?,
            Migration::Rust(migrate) => migrate(&tx)?,
        }
        tx.pragma_update(None, "user_version", reached)?;
        tx.commit()?;
    }
    Ok(())
}

/// How many migrations have run on this database.
pub(super) fn schema_version(conn: &Connection) -> rusqlite::Result<usize> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    // Negative isn't something this code ever writes, so it counts as newer.
    Ok(usize::try_from(version).unwrap_or(usize::MAX))
}
