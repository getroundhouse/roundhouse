use std::ffi::OsString;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use rusqlite::{Connection, params};
use tracing::warn;

use super::keys::{Facet, Subject};
use crate::store::columns::{ByColumn, Columns, OneValue, columns};

/// The form of what the file keeps. Another, as a build that kept another form left it, is emptied, and the cache is
/// read from GitHub again, rather than migrated. Raise it with any change to the table, or to the form of what's
/// written in `said`.
pub(super) const VERSION: i64 = 3;

/// The file the cache is kept in, so a hub that starts again has it. Writes wait for no sync, since a power cut that
/// takes the last of them back loses only what GitHub can say again.
pub(super) struct CacheFile {
    /// `None` where the file can't be made, so the cache is in memory alone, which a hub runs whole without, only
    /// reading GitHub more after it starts again.
    conn: Mutex<Option<Connection>>,
}

/// One facet of an issue or a pull request, as the file keeps it: what GitHub said, as JSON, and when it was read.
pub(super) struct Row {
    pub(super) subject: Subject,
    pub(super) facet: Facet,
    pub(super) said: String,
    pub(super) read_at_ms: i64,
}

columns! {
    /// The columns [`CacheFile::load`] reads of each facet the file keeps.
    struct FacetColumns {
        repository, subject, facet, said, read_at_ms,
    }
}

/// Why the file couldn't be opened.
#[derive(Debug, thiserror::Error)]
enum OpenError {
    #[error("making it: {0}")]
    Making(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

impl CacheFile {
    /// The file at `path`, emptied if it's of another form than [`VERSION`]. One that can't be opened, as one that's
    /// been damaged, is made again, and where even that fails, the cache is kept in memory alone, as the log says.
    pub(super) fn open(path: &Path) -> Self {
        let conn = opened(path).or_else(|error| {
            warn!("making {} again, since it couldn't be opened: {error}", path.display());
            for path in [path.to_path_buf(), suffixed(path, "-wal"), suffixed(path, "-shm")] {
                if let Err(error) = std::fs::remove_file(&path)
                    && error.kind() != std::io::ErrorKind::NotFound
                {
                    warn!("removing {}: {error}", path.display());
                }
            }
            opened(path)
        });
        let conn = conn
            .inspect_err(|error| {
                warn!(
                    "keeping what's read from GitHub in memory alone, since {} can't be made: {error}",
                    path.display()
                );
            })
            .ok();
        Self { conn: Mutex::new(conn) }
    }

    /// What it keeps that was read at or after `since_ms`, after it forgets the rest. A row it can't read, which
    /// only a file of its own version could hold, is left out.
    pub(super) fn load(&self, since_ms: i64) -> rusqlite::Result<Vec<Row>> {
        let conn = self.conn.lock();
        let Some(conn) = conn.as_ref() else { return Ok(Vec::new()) };
        conn.execute("DELETE FROM facets WHERE read_at_ms < ?", [since_ms])?;
        let mut statement = conn.prepare(&format!("SELECT {} FROM facets", FacetColumns::LIST))?;
        let c = FacetColumns::at(0);
        let rows = statement.query_map([], |row| {
            let subject = Subject::from_kept(row.at(c.repository)?, &row.at::<String>(c.subject)?);
            let facet = Facet::named(&row.at::<String>(c.facet)?);
            let (said, read_at_ms) = (row.at(c.said)?, row.at(c.read_at_ms)?);
            Ok(subject.zip(facet).map(|(subject, facet)| Row { subject, facet, said, read_at_ms }))
        })?;
        rows.filter_map(Result::transpose).collect()
    }

    /// Forgets each of `forgotten`, a facet of what it's about, and keeps each of `rows` in place of what was kept of its
    /// facet, all at once, so a pull request's facets are never from two reads, even after a crash part way.
    pub(super) fn keep(&self, forgotten: &[(Subject, Facet)], rows: &[Row]) -> rusqlite::Result<()> {
        let mut conn = self.conn.lock();
        let Some(conn) = conn.as_mut() else { return Ok(()) };
        let tx = conn.transaction()?;
        for (subject, facet) in forgotten {
            tx.execute(
                "DELETE FROM facets WHERE repository = ?1 AND subject = ?2 AND facet = ?3",
                params![subject.repository(), subject.key(), facet.name()],
            )?;
        }
        for row in rows {
            tx.execute(
                "INSERT INTO facets (repository, subject, facet, said, read_at_ms) VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT (repository, subject, facet) DO UPDATE SET said = excluded.said,
                         read_at_ms = excluded.read_at_ms",
                params![row.subject.repository(), row.subject.key(), row.facet.name(), row.said, row.read_at_ms],
            )?;
        }
        tx.commit()
    }
}

/// The file at `path`, with the table of this [`VERSION`], made, or emptied and made again where it's of another.
fn opened(path: &Path) -> Result<Connection, OpenError> {
    // Made here so it has its mode before SQLite opens it, which gives its -wal and -shm files the same one, since what
    // GitHub says of a private repository is private too.
    private_files::file_options().create(true).append(true).open(path)?;
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    // The hub before this one, from another build, can still be writing to it as this one starts.
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let version: i64 = conn.pragma_query_value(None, "user_version", OneValue::read)?;
    if version != VERSION {
        conn.execute_batch(&format!(
            "BEGIN;
             DROP TABLE IF EXISTS facets;
             CREATE TABLE facets (
                 repository TEXT NOT NULL,
                 subject TEXT NOT NULL,
                 facet TEXT NOT NULL,
                 said TEXT NOT NULL,
                 read_at_ms INTEGER NOT NULL,
                 PRIMARY KEY (repository, subject, facet)
             ) WITHOUT ROWID;
             PRAGMA user_version = {VERSION};
             COMMIT;"
        ))?;
    }
    Ok(conn)
}

/// `path` with `suffix` after its name, as SQLite names a database's -wal and -shm files.
fn suffixed(path: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(path.as_os_str());
    name.push(suffix);
    PathBuf::from(name)
}
