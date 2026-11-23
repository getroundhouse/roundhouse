use std::sync::Arc;

use rusqlite::{Connection, TransactionBehavior, params};

use super::keys::{Facet, Subject};
use crate::cache_file::{CacheFile, Part, drop_named};
use crate::store::columns::{ByColumn, Columns, OneValue, columns};

/// The form of what GitHub's part of the cache file keeps, which is the file's `user_version`. Another, as a build that
/// kept another form left it, is emptied, and the cache is read from GitHub again, rather than migrated. Raise it with
/// any change to the table, or to the form of what's written in `said`.
pub(super) const VERSION: i64 = 4;

/// GitHub's part of the cache file.
pub(super) struct GithubFile(Arc<CacheFile>);

/// One facet of an issue or a pull request, as the file keeps it: what GitHub said, as JSON, and when it was read.
pub(super) struct Row {
    pub(super) subject: Subject,
    pub(super) facet: Facet,
    pub(super) said: String,
    pub(super) read_at_ms: i64,
}

columns! {
    /// The columns [`GithubFile::load`] reads of each facet the file keeps.
    struct FacetColumns {
        repository, subject, facet, said, read_at_ms,
    }
}

/// GitHub's part of the cache file, under its own [`VERSION`].
pub(crate) static PART: Part = Part { name: "GitHub's cache", prepare, empty };

/// Makes GitHub's table of this [`VERSION`], emptying it where the file's is another.
fn prepare(conn: &mut Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", OneValue::read)?;
    if version != VERSION {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(&format!(
            "DROP TABLE IF EXISTS facets;
             CREATE TABLE facets (
                 repository TEXT NOT NULL,
                 subject TEXT NOT NULL,
                 facet TEXT NOT NULL,
                 said TEXT NOT NULL,
                 read_at_ms INTEGER NOT NULL,
                 PRIMARY KEY (repository, subject, facet)
             ) WITHOUT ROWID;
             PRAGMA user_version = {VERSION};"
        ))?;
        tx.commit()?;
    }
    Ok(())
}

/// Removes GitHub's table, whatever its form, and its version, so [`prepare`] makes it again.
fn empty(conn: &Connection) -> rusqlite::Result<()> {
    drop_named(conn, ["facets"])?;
    conn.pragma_update(None, "user_version", 0)
}

impl GithubFile {
    /// GitHub's part of `file`, opened with [`PART`].
    pub(super) fn new(file: Arc<CacheFile>) -> Self {
        Self(file)
    }

    /// What it keeps that was read at or after `since_ms`, after it forgets the rest. A row it can't read, which
    /// only a file of its own version could hold, is left out.
    pub(super) fn load(&self, since_ms: i64) -> rusqlite::Result<Vec<Row>> {
        let rows = self.0.with(&PART, |conn| {
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
        })?;
        Ok(rows.unwrap_or_default())
    }

    /// Forgets each of `forgotten`, a facet of what it's about, and keeps each of `rows` in place of what was kept of its
    /// facet, all at once, so a pull request's facets are never from two reads, even after a crash part way.
    pub(super) fn keep(&self, forgotten: &[(Subject, Facet)], rows: &[Row]) -> rusqlite::Result<()> {
        self.0.with(&PART, |conn| {
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
        })?;
        Ok(())
    }
}
