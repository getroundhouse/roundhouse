//! Copies of the database from before it migrates. A build from before a migration can't open what the
//! migration leaves, so a copy is the way back to it.

use std::cmp::Reverse;
use std::fmt;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use tracing::warn;

use super::error::BackupError;

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
    if dir.is_dir() {
        return Ok(());
    }
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| BackupError::io("creating the directory", e))?;
    dir.parent().map_or(Ok(()), |data_dir| sync_dir(data_dir, "syncing the data directory"))
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
    sync_dir(dir, "syncing the directory")
}

/// Copies through `conn`, so the copy has what's still in the WAL, which copying `hub.db` wouldn't.
fn copy_into(conn: &Connection, unfinished: &Path) -> Result<(), BackupError> {
    // Only this user can read it, like the database. SQLite would make a file others can read, but
    // `VACUUM INTO` writes into an empty one as it finds it.
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(unfinished)
        .map_err(|e| BackupError::io(format!("creating {UNFINISHED}"), e))?;
    let target = unfinished.to_str().ok_or_else(|| {
        BackupError::io("giving SQLite the path", io::Error::new(io::ErrorKind::InvalidFilename, "it isn't UTF-8"))
    })?;
    conn.execute("VACUUM INTO ?1", [target]).map_err(BackupError::Database)?;
    // SQLite syncs the copy as `synchronous` says, which on macOS is an `fsync` that can leave it in the
    // drive's cache. `sync_all` is `F_FULLFSYNC` there, so the copy is on disk before anything migrates.
    file.sync_all().map_err(|e| BackupError::io(format!("syncing {UNFINISHED}"), e))
}

fn sync_dir(dir: &Path, what: &str) -> Result<(), BackupError> {
    File::open(dir).and_then(|handle| handle.sync_all()).map_err(|e| BackupError::io(what, e))
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
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};

    use super::*;

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

    #[test]
    fn a_copy_that_fails_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let conn = Connection::open(dir.path().join("hub.db")).unwrap();
        let backups = dir.path().join("backups");
        // SQLite won't vacuum inside a transaction, so the copy fails once its file exists.
        conn.execute_batch("BEGIN").unwrap();

        let result = back_up(&conn, 1, &backups);

        assert!(matches!(result, Err(BackupError::Database(_))), "got {result:?}");
        assert_eq!(files_in(&backups), Vec::<String>::new());
    }

    #[test]
    fn only_its_owner_can_read_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let conn = Connection::open(dir.path().join("hub.db")).unwrap();

        let path = back_up(&conn, 1, &dir.path().join("backups")).unwrap();

        assert_eq!(fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o600);
    }
}
