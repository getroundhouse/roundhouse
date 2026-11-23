//! The file the hub keeps its copies of what others own in, apart from its database: what it last read from GitHub,
//! and its observations. The hub empties it rather than migrates it, as `docs/principles/observations.md` says, and
//! each part of it says the form of its own tables, so a build that changes one part's form empties only that part.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use rusqlite::Connection;
use tracing::warn;

/// The file, so a hub that starts again has what it kept. Writes wait for no sync, since a power cut that takes the
/// last of them back loses only what can be read again, or what's then as of an earlier read.
pub(crate) struct CacheFile {
    /// `None` where the file can't be made, so what it would keep is in memory alone, which a hub runs whole without,
    /// only knowing less after it starts again.
    conn: Mutex<Option<Connection>>,
}

/// Makes one part's tables of the forms this build keeps, emptying any of another form.
pub(crate) type Prepare = fn(&Connection) -> rusqlite::Result<()>;

/// Why the file couldn't be opened.
#[derive(Debug, thiserror::Error)]
enum OpenError {
    #[error("making it: {0}")]
    Making(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

impl CacheFile {
    /// The file at `path`, with the tables each of `parts` makes. One that can't be opened or prepared, as one that's
    /// been damaged, is made again, and where even that fails, what it would keep is in memory alone, as the log says.
    pub(crate) fn open(path: &Path, parts: &[Prepare]) -> Self {
        let conn = opened(path, parts).or_else(|error| {
            warn!("making {} again, since it couldn't be opened: {error}", path.display());
            for path in [path.to_path_buf(), suffixed(path, "-wal"), suffixed(path, "-shm")] {
                if let Err(error) = std::fs::remove_file(&path)
                    && error.kind() != std::io::ErrorKind::NotFound
                {
                    warn!("removing {}: {error}", path.display());
                }
            }
            opened(path, parts)
        });
        let conn = conn
            .inspect_err(|error| {
                warn!("keeping in memory alone what {} would keep, since it can't be made: {error}", path.display());
            })
            .ok();
        Self { conn: Mutex::new(conn) }
    }

    /// What `f` makes of the file, or `None` where there's no file, so what it would keep is in memory alone. Blocks on
    /// the file, and on whatever else is using it meanwhile.
    pub(crate) fn with<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> rusqlite::Result<T>,
    ) -> rusqlite::Result<Option<T>> {
        let mut conn = self.conn.lock();
        conn.as_mut().map(f).transpose()
    }
}

/// The file at `path`, with the tables each of `parts` makes.
fn opened(path: &Path, parts: &[Prepare]) -> Result<Connection, OpenError> {
    // Made here so it has its mode before SQLite opens it, which gives its -wal and -shm files the same one, since what
    // GitHub says of a private repository is private too, and so is what a workspace holds.
    private_files::file_options().create(true).append(true).open(path)?;
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    // The hub before this one, from another build, can still be writing to it as this one starts.
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    for prepare in parts {
        prepare(&conn)?;
    }
    Ok(conn)
}

/// `path` with `suffix` after its name, as SQLite names a database's -wal and -shm files.
fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(path.as_os_str());
    name.push(suffix);
    PathBuf::from(name)
}
