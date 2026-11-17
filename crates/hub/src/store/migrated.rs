//! A database that every migration has run on, so a test's store starts as a copy of it rather than running all of them
//! from nothing, which was most of what a typical test took. The hub's own tests copy it, and through
//! [`give_a_migrated_database`], so do those that start a hub from outside, before it starts.
//!
//! nextest, which CI runs the tests with, starts a process for each test, so one made in each process would be made for
//! every test. It's kept beside the test binary instead, named for the build of the binary, as
//! [`process::testing::kept`] has it, and each process reads it.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::Path;
use std::sync::LazyLock;

use super::{Store, StoreError};

/// What `hub.db` holds once [`Store::open`] has made it and run every migration on it. Read from what this build of the
/// test binary keeps, or made, and kept for the next process of this build. Any other build is another binary, so it
/// can't read what one with other migrations kept, and when one of this build's migrations fails, so does every test
/// that would copy it.
///
/// Every copy has the hub ID a migration chose at random for this one, so a test of what that migration gives opens a
/// store of its own with [`Store::open`].
static MIGRATED: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let kept = process::testing::kept("migrated.db", |path| migrate(path).map_err(std::io::Error::other));
    if let Some(bytes) = kept.and_then(|path| fs::read(path).ok()) {
        return bytes;
    }
    // Made where nothing else looks, and let go of once it's read.
    let dir = std::env::temp_dir().join(format!("hub-migrating-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("couldn't make {}: {e}", dir.display()));
    let path = dir.join("hub.db");
    migrate(&path).unwrap_or_else(|e| panic!("couldn't make a database with every migration: {e}"));
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("couldn't read {}: {e}", path.display()));
    let _ = fs::remove_dir_all(dir);
    bytes
});

/// Puts a copy of the migrated database at `path`, unless a database is there already, as when a test opens its store
/// again, or opens one an older build left. [`Store::open`] then finds nothing to migrate in the copy. Only this user's,
/// as the hub makes its own.
pub(crate) fn copy_to(path: &Path) {
    let migrated: &[u8] = &MIGRATED;
    match private_files::file_options().write(true).create_new(true).open(path) {
        Ok(mut file) => file.write_all(migrated).unwrap_or_else(|e| panic!("couldn't write {}: {e}", path.display())),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
        Err(e) => panic!("couldn't make {}: {e}", path.display()),
    }
}

/// Puts a copy of the migrated database where a hub that's to start with `paths` keeps its own, unless one is there, in
/// a data directory made as the hub, and a client that starts one, make it. The hub then opens it with its real
/// [`Store::open`], which finds nothing to migrate. For a test that starts a hub from outside and isn't about what the
/// migrations make, before the hub starts. Every hub given one has the same ID, which a migration chose at random for the database.
#[cfg(feature = "testing")]
pub fn give_a_migrated_database(paths: &protocol::paths::Paths) {
    let dir = &paths.data_dir;
    private_files::create_dir_all(dir).unwrap_or_else(|e| panic!("couldn't make {}: {e}", dir.display()));
    copy_to(&paths.database);
}

/// Makes the database at `path`, with what's in its log moved into it once its store has closed. A database made from
/// nothing has nothing to back up, so nothing is written to the backups it's given.
fn migrate(path: &Path) -> Result<(), StoreError> {
    drop(Store::open(path, &path.with_extension("backups"))?);
    let mut log = path.as_os_str().to_owned();
    log.push("-wal");
    assert!(!Path::new(&log).exists(), "the store's log is left beside it, so the database isn't whole");
    Ok(())
}
