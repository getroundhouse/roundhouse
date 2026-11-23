use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use parking_lot::Mutex;
use rusqlite::Connection;
use tracing::warn;

use super::failure::Failure;
use super::parts::{Part, prepared_or_emptied};

/// How long the hub waits for the file while another connection holds it, as the hub before this one can as it stops,
/// before it leaves it as it is. That hub writes a few rows at a time, so this is reached only where it's stuck.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// The file, so a hub that starts again has what it kept. Writes wait for no sync, since a power cut that takes the
/// last of them back loses only what can be read again, or what's then as of an earlier read.
pub(crate) struct CacheFile {
    /// `None` where the file can't be opened, so what it would keep is in memory alone, which a hub runs whole without,
    /// only knowing less after it starts again.
    conn: Mutex<Option<Connection>>,
    /// The name of each part whose tables are of this build's form, which alone is read and written.
    prepared: Vec<&'static str>,
}

/// Why the file couldn't be opened.
#[derive(Debug, thiserror::Error)]
enum OpenError {
    #[error("making it: {0}")]
    Making(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

impl OpenError {
    /// What it says of the file. One that couldn't be made or opened for writing, as on a full disk or in a directory
    /// the hub can't write, can't be used, which says nothing of what's in it.
    fn failure(&self) -> Failure {
        match self {
            Self::Making(_) => Failure::Unusable,
            Self::Sqlite(error) => Failure::of(error),
        }
    }
}

impl CacheFile {
    /// The file at `path`, with the tables of each of `parts`, as [`CacheFile::open_waiting`] says.
    pub(crate) fn open(path: &Path, parts: &[&Part]) -> Self {
        Self::open_waiting(path, parts, BUSY_TIMEOUT)
    }

    /// The file at `path`, with the tables of each of `parts`, waiting up to `busy_timeout` for another connection
    /// that holds it.
    ///
    /// A part whose tables can't be made is emptied and made again, which leaves the others as they are. A part the
    /// file was still held for once the wait ran out is neither read nor written until the hub starts again. Only a
    /// file SQLite says isn't a database, or is damaged, is made again, and where even that fails, what it would keep
    /// is in memory alone. One that can't be opened for any other reason, as where another holds it from the start, the
    /// disk is full or the hub can't write it, is left as it is, neither read nor written until the hub starts again,
    /// since it can hold what nothing else has. The log says each.
    pub(super) fn open_waiting(path: &Path, parts: &[&Part], busy_timeout: Duration) -> Self {
        let error = match opened(path, parts, busy_timeout) {
            Ok(opened) => return Self::of(opened),
            Err(error) => error,
        };
        match error.failure() {
            Failure::Broken => warn!("making {} again, since it couldn't be read: {error}", path.display()),
            Failure::Busy => {
                warn!(
                    "leaving {} unread and unwritten until the hub starts again, since another holds it: {error}",
                    path.display()
                );
                return Self::in_memory_alone();
            }
            Failure::Unusable | Failure::Other => {
                warn!(
                    "leaving {} as it is, unread and unwritten until the hub starts again, since it couldn't be \
                     opened: {error}",
                    path.display()
                );
                return Self::in_memory_alone();
            }
        }
        for path in [path.to_path_buf(), suffixed(path, "-wal"), suffixed(path, "-shm")] {
            if let Err(error) = std::fs::remove_file(&path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                warn!("removing {}: {error}", path.display());
            }
        }
        match opened(path, parts, busy_timeout) {
            Ok(opened) => Self::of(opened),
            Err(error) => {
                warn!(
                    "keeping in memory alone what {} would keep, since it can't be made again: {error}",
                    path.display()
                );
                Self::in_memory_alone()
            }
        }
    }

    /// The file [`opened`] made, with the name of each part whose tables it made.
    fn of((conn, prepared): (Connection, Vec<&'static str>)) -> Self {
        Self { conn: Mutex::new(Some(conn)), prepared }
    }

    /// No file, so what it would keep is in memory alone.
    fn in_memory_alone() -> Self {
        Self { conn: Mutex::new(None), prepared: Vec::new() }
    }

    /// What `f` makes of the file, or `None` where `part`'s tables weren't made, as where there's no file, so what
    /// they would keep is in memory alone. Blocks on the file, and on whatever else is using it meanwhile.
    pub(crate) fn with<T>(
        &self,
        part: &Part,
        f: impl FnOnce(&mut Connection) -> rusqlite::Result<T>,
    ) -> rusqlite::Result<Option<T>> {
        if !self.prepared.contains(&part.name) {
            return Ok(None);
        }
        let mut conn = self.conn.lock();
        conn.as_mut().map(f).transpose()
    }
}

/// The file at `path`, and the name of each of `parts` whose tables it made, as [`CacheFile::open_waiting`] says. Fails
/// where the file can't be opened, is held from the start, is broken or can't be used, which a part's failure can say
/// too.
fn opened(path: &Path, parts: &[&Part], busy_timeout: Duration) -> Result<(Connection, Vec<&'static str>), OpenError> {
    // Made here so it has its mode before SQLite opens it, which gives its -wal and -shm files the same one, since what
    // GitHub says of a private repository is private too, and so is what a workspace holds.
    private_files::file_options().create(true).append(true).open(path)?;
    let mut conn = Connection::open(path)?;
    // Before anything reads it, since the hub before this one, from another build, can still be using it as this one
    // starts.
    conn.busy_timeout(busy_timeout)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    let mut prepared = Vec::new();
    for part in parts {
        let Err(error) = prepared_or_emptied(&mut conn, part.name, part.prepare, part.empty) else {
            prepared.push(part.name);
            continue;
        };
        match Failure::of(&error) {
            // What a part can't read or write, the others can't either.
            Failure::Broken | Failure::Unusable => return Err(error.into()),
            Failure::Busy => warn!(
                "leaving {} in {} unread and unwritten until the hub starts again, since another holds the file: \
                 {error}",
                part.name,
                path.display()
            ),
            Failure::Other => warn!(
                "leaving {} in {} unread and unwritten, since its tables can't be made even once emptied: {error}",
                part.name,
                path.display()
            ),
        }
    }
    Ok((conn, prepared))
}

/// `path` with `suffix` after its name, as SQLite names a database's -wal and -shm files.
fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(path.as_os_str());
    name.push(suffix);
    PathBuf::from(name)
}
