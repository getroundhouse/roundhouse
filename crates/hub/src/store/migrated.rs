//! A database that every migration has run on, so a test's store starts as a copy of it rather than running all of them
//! from nothing, which was most of what a typical test took.
//!
//! nextest, which CI runs the tests with, starts a process for each test, so one made in each process would be made for
//! every test. It's kept beside the test binary instead, named for the build of the binary, and each process reads it.

use std::fs::{self, OpenOptions};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use super::Store;

/// What `hub.db` holds once [`Store::open`] has made it and run every migration on it. Read from what this build of the
/// test binary keeps, or made, and kept for the next process of this build. Any other build is another binary, so it
/// can't read what one with other migrations kept, and when one of this build's migrations fails, so does every test
/// that would copy it.
///
/// Every copy has the hub ID a migration chose at random for this one, so a test of what that migration gives opens a
/// store of its own with [`Store::open`].
static MIGRATED: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let kept = kept_at();
    if let Some(bytes) = kept.as_deref().and_then(|path| fs::read(path).ok()) {
        return bytes;
    }
    let bytes = migrate();
    if let Some(path) = kept {
        keep(&path, &bytes);
    }
    bytes
});

/// Puts a copy of the migrated database at `path`, unless a database is there already, as when a test opens its store
/// again, or opens one an older build left. [`Store::open`] then finds nothing to migrate in the copy.
pub(crate) fn copy_to(path: &Path) {
    let migrated: &[u8] = &MIGRATED;
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => file.write_all(migrated).unwrap(),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
        Err(e) => panic!("couldn't make {}: {e}", path.display()),
    }
}

/// Makes the database, and reads it once its store has closed, so what was in its log is in it.
fn migrate() -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    drop(Store::open(&path, &dir.path().join("backups")).unwrap());
    assert!(!path.with_extension("db-wal").exists(), "the store's log is left beside it, so the database isn't whole");
    fs::read(&path).unwrap()
}

/// Where this build of the test binary keeps the database: beside it, named for its size and when it was written, which
/// a build that changes anything, a migration included, changes. `None` when the binary can't be found.
fn kept_at() -> Option<PathBuf> {
    let binary = std::env::current_exe().ok()?;
    let written = fs::metadata(&binary).ok()?;
    let mut build = DefaultHasher::new();
    (written.len(), written.modified().ok()?).hash(&mut build);
    let name = binary.file_name()?.to_str()?;
    Some(binary.with_file_name(format!("{name}.{:016x}.migrated.db", build.finish())))
}

/// Keeps `bytes` at `path` for the processes after this one, and lets go of what earlier builds kept. Written beside it
/// and renamed into place, so another process reads all of it or none. Nothing kept only means the next process makes
/// it again, so a write that fails is let be.
fn keep(path: &Path, bytes: &[u8]) {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|name| name.to_str())) else { return };
    let writing = path.with_extension(format!("{}.writing", std::process::id()));
    if fs::write(&writing, bytes).and_then(|()| fs::rename(&writing, path)).is_err() {
        let _ = fs::remove_file(&writing);
        return;
    }
    let binary = name.split('.').next().unwrap_or(name);
    let earlier = fs::read_dir(dir).into_iter().flatten().flatten().map(|entry| entry.path()).filter(|other| {
        let other_name = other.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        other != path && other_name.starts_with(&format!("{binary}.")) && other_name.ends_with(".migrated.db")
    });
    for other in earlier {
        let _ = fs::remove_file(other);
    }
}
