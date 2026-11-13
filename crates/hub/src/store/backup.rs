//! Copies of the database from before it migrates. A build from before a migration can't open what the
//! migration leaves, so a copy is the way back to it.

use std::cmp::Reverse;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use tracing::warn;

use super::error::BackupError;
use crate::private;

/// How many schema versions keep a copy. Each copy is a whole database, so they can't pile up, and three
/// steps back through the last three updates that migrated.
const KEPT: usize = 3;

/// Where a copy is written before it gets its name, so a file with a backup's name is always complete.
const UNFINISHED: &str = "unfinished.db";

/// Saves a copy of the database, which is at schema `version`, into `dir`, and then prunes the older ones.
/// Returns where the copy is.
pub(crate) fn back_up(conn: &Connection, version: usize, dir: &Path) -> Result<PathBuf, BackupError> {
    create_dir(dir)?;
    let mut names = list(dir)?;
    let number = names.iter().map(|name| name.number).max().map_or(1, |last| last.saturating_add(1));
    let name = Name { version, number };
    write(conn, dir, name)?;
    names.push(name);
    prune(dir, names);
    Ok(dir.join(name.to_string()))
}

/// Only this user can open it, like the data directory. Making it syncs the data directory, so it's still
/// there, with the copies in it, after a power cut.
fn create_dir(dir: &Path) -> Result<(), BackupError> {
    private::create_dir_lasting(dir).map_err(|e| BackupError::io("creating the directory", e))
}

/// Writes the copy under [`UNFINISHED`], then gives it its name, and syncs that before anything migrates.
fn write(conn: &Connection, dir: &Path, name: Name) -> Result<(), BackupError> {
    let unfinished = dir.join(UNFINISHED);
    // Only a hub stopped part way through a copy leaves one, since a copy that fails removes its own. The
    // hub's lock means no other hub is writing it.
    if let Err(e) = fs::remove_file(&unfinished)
        && e.kind() != io::ErrorKind::NotFound
    {
        return Err(BackupError::io(format!("removing the {UNFINISHED} left by an earlier copy"), e));
    }
    let named = copy_into(conn, &unfinished).and_then(|()| {
        fs::rename(&unfinished, dir.join(name.to_string()))
            .map_err(|e| BackupError::io(format!("renaming {UNFINISHED} to {name}"), e))
    });
    if named.is_err() {
        // It can be as big as the database, on a disk that may be full.
        let _ = fs::remove_file(&unfinished);
    }
    named?;
    private::sync_dir(dir).map_err(|e| BackupError::io("syncing the directory", e))
}

/// Copies the database, with what's still in its WAL, which a copy of `hub.db` alone wouldn't have. Once the WAL is
/// all in `hub.db`, the file is the whole database, and nothing else writes it before the migration does, since the
/// hub's lock keeps other hubs out, and a hub stops writing before it lets go of it. So a copy of the file is enough.
/// Otherwise SQLite copies it, which reads and writes every row: seconds for a database of 2 GB, which someone waits
/// through as the app connects after an update.
fn copy_into(conn: &Connection, unfinished: &Path) -> Result<(), BackupError> {
    match whole_file(conn)? {
        Some(file) => copy_file(file, unfinished),
        None => vacuum_into(conn, unfinished),
    }
}

/// The database's file, once everything in the WAL has been moved into it. `None` when it couldn't all be moved
/// without waiting, as while another program reads an older state of the database, which the WAL keeps for it, or
/// moves it itself, or when the database has no file.
fn whole_file(conn: &Connection) -> Result<Option<&Path>, BackupError> {
    // rusqlite's path is optional, and empty for a database in memory, which still gets a copy.
    let Some(file) = conn.path().filter(|path| !path.is_empty()) else {
        return Ok(None);
    };
    // A passive checkpoint never waits, where the others wait out the connection's busy timeout for a reader holding
    // the WAL back, before the copy could go through SQLite. Blocked, as by another checkpoint, both counts are -1,
    // and they're -1 for a database not in WAL mode, whose file is the whole of it.
    let (blocked, in_wal, moved): (bool, i64, i64) = conn
        .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(BackupError::Checkpoint)?;
    Ok((!blocked && in_wal == moved).then_some(Path::new(file)))
}

/// Copies `file`, which `fs::copy` does with a clone where the file system can make one, as APFS and btrfs can: at
/// once, with the two sharing what's on disk until either changes. Windows copies a file SQLite has open and locked,
/// as the hub's connection has `hub.db`, with the locks past 1 GiB inside it.
fn copy_file(file: &Path, unfinished: &Path) -> Result<(), BackupError> {
    fs::copy(file, unfinished)
        .map_err(|e| BackupError::io(format!("copying {} to {UNFINISHED}", file.display()), e))?;
    // A copy takes the mode of the file it copies, and a backup is only this user's, whatever that is.
    private::restrict(unfinished).map_err(|e| BackupError::io(format!("making {UNFINISHED} private"), e))?;
    // `sync_all` is `F_FULLFSYNC` on macOS, so the copy is on disk before anything migrates. Windows only flushes a
    // file through a handle that can write it.
    OpenOptions::new()
        .write(true)
        .open(unfinished)
        .and_then(|copy| copy.sync_all())
        .map_err(|e| BackupError::io(format!("syncing {UNFINISHED}"), e))
}

/// Copies through `conn`, which reads the whole database, what's in the WAL included, and writes every row again.
fn vacuum_into(conn: &Connection, unfinished: &Path) -> Result<(), BackupError> {
    // Only this user can read it, like the database. SQLite would make a file others can read, but
    // `VACUUM INTO` writes into an empty one as it finds it.
    let file = private::file_options()
        .write(true)
        .create_new(true)
        .open(unfinished)
        .map_err(|e| BackupError::io(format!("creating {UNFINISHED}"), e))?;
    let target = unfinished.to_str().ok_or_else(|| {
        BackupError::io("giving SQLite the path", io::Error::new(io::ErrorKind::InvalidFilename, "it isn't UTF-8"))
    })?;
    conn.execute("VACUUM INTO ?1", [target]).map_err(BackupError::Vacuum)?;
    // SQLite syncs the copy as `synchronous` says, which on macOS is an `fsync` that can leave it in the
    // drive's cache. `sync_all` is `F_FULLFSYNC` there, so the copy is on disk before anything migrates.
    file.sync_all().map_err(|e| BackupError::io(format!("syncing {UNFINISHED}"), e))
}

/// Keeps the newest copy of each of the [`KEPT`] versions copied most recently, and removes the rest. Each
/// start that retries an update that stopped part way copies the same version again, so it replaces that
/// version's copy, rather than pushing out the one from before the update. A copy that can't be removed
/// only costs space, so it doesn't stop the migration.
fn prune(dir: &Path, mut names: Vec<Name>) {
    names.sort_unstable_by_key(|name| Reverse(name.number));
    let mut versions = Vec::with_capacity(KEPT);
    for name in names {
        if versions.len() < KEPT && !versions.contains(&name.version) {
            versions.push(name.version);
        } else if let Err(e) = fs::remove_file(dir.join(name.to_string())) {
            warn!(backup = %name, "couldn't remove an old copy of the database: {e}");
        }
    }
}

/// The backups in `dir`. Anything else there is left alone.
fn list(dir: &Path) -> Result<Vec<Name>, BackupError> {
    let listing = |e| BackupError::io("listing what's in the directory", e);
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).map_err(listing)? {
        if let Some(name) = entry.map_err(listing)?.file_name().to_str().and_then(Name::parse) {
            names.push(name);
        }
    }
    Ok(names)
}

/// A backup's file name: the schema version it holds, then a number that counts up across every backup,
/// so two of one version are kept apart, and the newest is the highest whatever the clock says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Name {
    version: usize,
    number: u64,
}

impl Name {
    /// Only takes a name this writes, so a backup it lists is one it can find again by name.
    fn parse(file_name: &str) -> Option<Self> {
        let (version, number) = file_name.strip_prefix("hub-v")?.strip_suffix(".db")?.split_once('-')?;
        let name = Self { version: version.parse().ok()?, number: number.parse().ok()? };
        (name.to_string() == file_name).then_some(name)
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "hub-v{}-{}.db", self.version, self.number)
    }
}

/// What's in `dir`, by name, in order, so a test can see everything a copy left. Nothing, if `dir` isn't there.
#[cfg(test)]
pub(crate) fn files_in(dir: &Path) -> Vec<String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return vec![],
        Err(e) => panic!("reading {}: {e}", dir.display()),
    };
    let mut names: Vec<String> = entries.map(|entry| entry.unwrap().file_name().into_string().unwrap()).collect();
    names.sort();
    names
}

#[cfg(test)]
mod tests;
