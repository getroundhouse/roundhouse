//! Each kind's table in the cache file, under a version of its own.

use rusqlite::{Connection, OptionalExtension, params};
use tracing::warn;

use super::kept::Tables;
use super::kinds::{Form, Kind};
use crate::cache_file::CacheFile;
use crate::store::columns::{ByColumn, Columns, OneValue, columns};

/// What the file kept of one observation.
pub(super) struct Row<K: Kind> {
    pub(super) of: K::Of,
    pub(super) value: K::Value,
    pub(super) read_at_ms: i64,
}

columns! {
    /// The columns [`load`] reads of each observation a kind's table keeps.
    struct ObservedColumns {
        of, said, read_at_ms,
    }
}

/// Makes each kind's table of the version this build keeps, emptying one of another, as [`CacheFile::open`] has each
/// part do. A table of a kind this build doesn't know, as a newer one left it, is left as it is.
pub(crate) fn prepare(conn: &Connection) -> rusqlite::Result<()> {
    prepare_forms(conn, &Tables::forms())
}

/// Makes a table of each of `forms`, as [`prepare`] says.
pub(super) fn prepare_forms(conn: &Connection, forms: &[Form]) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS observation_forms (
             observed TEXT PRIMARY KEY,
             version INTEGER NOT NULL
         ) WITHOUT ROWID;",
    )?;
    for Form { table, version } in forms {
        let kept: Option<i64> = conn
            .query_row("SELECT version FROM observation_forms WHERE observed = ?", [table], OneValue::read)
            .optional()?;
        if kept != Some(*version) {
            conn.execute_batch(&format!(
                "BEGIN;
                 DROP TABLE IF EXISTS observed_{table};
                 CREATE TABLE observed_{table} (
                     of TEXT PRIMARY KEY,
                     said TEXT NOT NULL,
                     read_at_ms INTEGER NOT NULL
                 ) WITHOUT ROWID;
                 INSERT INTO observation_forms (observed, version) VALUES ('{table}', {version})
                     ON CONFLICT (observed) DO UPDATE SET version = excluded.version;
                 COMMIT;"
            ))?;
        }
    }
    Ok(())
}

/// What the file keeps of kind `K`. A row it can't read, which only a damaged file could hold, since the table is of
/// this build's version, is left out.
pub(super) fn load<K: Kind>(file: &CacheFile) -> rusqlite::Result<Vec<Row<K>>> {
    let table = K::FORM.table;
    let rows = file.with(|conn| {
        let mut statement = conn.prepare(&format!("SELECT {} FROM observed_{table}", ObservedColumns::LIST))?;
        let c = ObservedColumns::at(0);
        let rows = statement.query_map([], |row| {
            let (of, said): (String, String) = (row.at(c.of)?, row.at(c.said)?);
            let read_at_ms = row.at(c.read_at_ms)?;
            let read = serde_json::from_str(&of).ok().zip(serde_json::from_str(&said).ok());
            Ok(read.map(|(of, value)| Row { of, value, read_at_ms }))
        })?;
        rows.filter_map(Result::transpose).collect()
    })?;
    Ok(rows.unwrap_or_default())
}

/// Keeps that the observation of `of` says `value`, as read at `read_at_ms`, in place of what the file kept of it, or,
/// with no `value`, forgets it. A write the file refuses is only logged: what's in memory, which clients are told, goes
/// on as it is, and a hub that starts again has what the file kept before.
pub(super) fn write<K: Kind>(file: &CacheFile, of: &K::Of, said: Option<(&K::Value, i64)>) {
    if let Err(error) = written::<K>(file, of, said) {
        warn!(table = K::FORM.table, "keeping an observation in the cache's file: {error}");
    }
}

/// What [`write`] does, or why it couldn't.
fn written<K: Kind>(file: &CacheFile, of: &K::Of, said: Option<(&K::Value, i64)>) -> Result<(), WriteError> {
    let (table, of) = (K::FORM.table, serde_json::to_string(of)?);
    let said = said.map(|(value, read_at_ms)| serde_json::to_string(value).map(|value| (value, read_at_ms)));
    let said = said.transpose()?;
    file.with(|conn| match said {
        Some((value, read_at_ms)) => conn.execute(
            &format!(
                "INSERT INTO observed_{table} (of, said, read_at_ms) VALUES (?1, ?2, ?3)
                     ON CONFLICT (of) DO UPDATE SET said = excluded.said, read_at_ms = excluded.read_at_ms"
            ),
            params![of, value, read_at_ms],
        ),
        None => conn.execute(&format!("DELETE FROM observed_{table} WHERE of = ?"), [of]),
    })?;
    Ok(())
}

/// Why an observation couldn't be written to the file.
#[derive(Debug, thiserror::Error)]
enum WriteError {
    #[error("writing it as JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}
