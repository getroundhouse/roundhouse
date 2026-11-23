//! Each kind's table in the cache file, under a version of its own.

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use tracing::warn;

use super::kinds::{Form, Kind};
use super::tables::Tables;
use crate::cache_file::{CacheFile, Part, drop_named, prepared_or_emptied};
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

/// The observations' part of the cache file, with a table and a version for each kind.
pub(crate) static PART: Part = Part { name: "observations", prepare, empty };

/// Makes each kind's table of the version this build keeps, emptying one of another. A table of a kind this build
/// doesn't know, as a newer one left it, is left as it is.
fn prepare(conn: &mut Connection) -> rusqlite::Result<()> {
    prepare_forms(conn, &Tables::forms())
}

/// Removes every kind's table, of this build's kinds and any other, and the version of each, so [`prepare`] makes
/// them again, where the part's tables can't be made.
fn empty(conn: &Connection) -> rusqlite::Result<()> {
    let mut statement = conn.prepare(r"SELECT name FROM sqlite_schema WHERE name LIKE 'observed\_%' ESCAPE '\'")?;
    let tables: Vec<String> = statement.query_map([], OneValue::read)?.collect::<rusqlite::Result<_>>()?;
    drop_named(conn, tables.iter().map(String::as_str).chain(["observation_forms"]))
}

/// Makes a table of each of `forms`, as [`prepare`] says. One whose table can't be made, as where something of
/// another form has its name, is emptied and made again, which leaves the others as they are.
pub(super) fn prepare_forms(conn: &mut Connection, forms: &[Form]) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS observation_forms (
             observed TEXT PRIMARY KEY,
             version INTEGER NOT NULL
         ) WITHOUT ROWID;",
    )?;
    for form in forms {
        let what = format!("the observations of {}", form.table);
        prepared_or_emptied(conn, &what, |conn| prepare_form(conn, form), |conn| empty_form(conn, form.table))?;
    }
    Ok(())
}

/// Makes the table of `form`, emptying one of another version.
fn prepare_form(conn: &mut Connection, Form { table, version }: &Form) -> rusqlite::Result<()> {
    let kept: Option<i64> = conn
        .query_row("SELECT version FROM observation_forms WHERE observed = ?", [table], OneValue::read)
        .optional()?;
    if kept != Some(*version) {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(&format!(
            "DROP TABLE IF EXISTS observed_{table};
             CREATE TABLE observed_{table} (
                 of TEXT PRIMARY KEY,
                 said TEXT NOT NULL,
                 read_at_ms INTEGER NOT NULL
             ) WITHOUT ROWID;
             INSERT INTO observation_forms (observed, version) VALUES ('{table}', {version})
                 ON CONFLICT (observed) DO UPDATE SET version = excluded.version;"
        ))?;
        tx.commit()?;
    }
    Ok(())
}

/// Removes the table of kind `table`, whatever its form, and its version, so [`prepare_form`] makes it again.
fn empty_form(conn: &Connection, table: &str) -> rusqlite::Result<()> {
    drop_named(conn, [format!("observed_{table}").as_str()])?;
    conn.execute("DELETE FROM observation_forms WHERE observed = ?", [table])?;
    Ok(())
}

/// What the file keeps of kind `K`. A row it can't read, which only a damaged file could hold, since the table is of
/// this build's version, is left out.
pub(super) fn load<K: Kind>(file: &CacheFile) -> rusqlite::Result<Vec<Row<K>>> {
    let table = K::FORM.table;
    let rows = file.with(&PART, |conn| {
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

/// What a change to one observation writes to the file.
pub(super) enum Write<V> {
    /// That it says `value`, as read at `read_at_ms`, in place of what the file kept of it.
    Says { value: V, read_at_ms: i64 },
    /// Only that it was read again at `read_at_ms`, since it says what the file keeps, which isn't written again.
    ReadAgain { read_at_ms: i64 },
    /// That it's forgotten.
    Gone,
}

/// Writes `write` of the observation of `of`. A write the file refuses is only logged: what's in memory, which clients
/// are told, goes on as it is, and a hub that starts again has what the file kept before.
pub(super) fn write<K: Kind>(file: &CacheFile, of: &K::Of, write: Write<K::Value>) {
    if let Err(error) = written::<K>(file, of, write) {
        warn!(table = K::FORM.table, "keeping an observation in the cache's file: {error}");
    }
}

/// What [`write`] does, or why it couldn't.
fn written<K: Kind>(file: &CacheFile, of: &K::Of, write: Write<K::Value>) -> Result<(), WriteError> {
    let (table, of) = (K::FORM.table, serde_json::to_string(of)?);
    let write = match write {
        Write::Says { value, read_at_ms } => Write::Says { value: serde_json::to_string(&value)?, read_at_ms },
        Write::ReadAgain { read_at_ms } => Write::ReadAgain { read_at_ms },
        Write::Gone => Write::Gone,
    };
    file.with(&PART, |conn| match write {
        Write::Says { value, read_at_ms } => conn.execute(
            &format!(
                "INSERT INTO observed_{table} (of, said, read_at_ms) VALUES (?1, ?2, ?3)
                     ON CONFLICT (of) DO UPDATE SET said = excluded.said, read_at_ms = excluded.read_at_ms"
            ),
            params![of, value, read_at_ms],
        ),
        Write::ReadAgain { read_at_ms } => {
            conn.execute(&format!("UPDATE observed_{table} SET read_at_ms = ?2 WHERE of = ?1"), params![of, read_at_ms])
        }
        Write::Gone => conn.execute(&format!("DELETE FROM observed_{table} WHERE of = ?"), [of]),
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
