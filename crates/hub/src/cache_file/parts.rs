use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use tracing::warn;

use super::failure::Failure;
use crate::store::columns::OneValue;

/// One part of the file, whose tables are of a form it keeps a version of, so the hub empties a part apart from the
/// others, where a build changes its form or its tables can't be made.
pub(crate) struct Part {
    /// What the log calls it.
    pub(crate) name: &'static str,
    /// Makes the part's tables of the forms this build keeps, emptying any of another form. Its writes go in
    /// transactions of its own, so one that fails leaves nothing half made.
    pub(crate) prepare: fn(&mut Connection) -> rusqlite::Result<()>,
    /// Removes each of the part's tables, whatever its form, and what says the form of each, so
    /// [`prepare`](Part::prepare) makes them all again. It runs in a transaction it's given.
    pub(crate) empty: fn(&Connection) -> rusqlite::Result<()>,
}

/// Runs `prepare` on `conn`, and where it fails because of what's in the file, as on a table of a form this build
/// can't make its own over, runs `empty`, which removes what `prepare` makes, and then `prepare` again, so what can't
/// be made loses only its own rows. A failure because the file is busy, broken or can't be used, which emptying can't
/// help, is returned as it is, with nothing emptied.
pub(crate) fn prepared_or_emptied(
    conn: &mut Connection,
    what: &str,
    prepare: impl Fn(&mut Connection) -> rusqlite::Result<()>,
    empty: impl FnOnce(&Connection) -> rusqlite::Result<()>,
) -> rusqlite::Result<()> {
    let Err(error) = prepare(conn) else { return Ok(()) };
    match Failure::of(&error) {
        Failure::Busy | Failure::Broken | Failure::Unusable => return Err(error),
        Failure::Other => warn!("emptying {what} in the cache file, since its tables couldn't be made: {error}"),
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    empty(&tx)?;
    tx.commit()?;
    prepare(conn)
}

/// Drops what's named each of `names` in the file, whether a table, a view, an index or a trigger, as a build with
/// another form could have left there, and nothing where there's none.
pub(crate) fn drop_named<'a>(conn: &Connection, names: impl IntoIterator<Item = &'a str>) -> rusqlite::Result<()> {
    for name in names {
        // Looked up again for each, since dropping a table drops its indexes and triggers too.
        let kind: Option<String> =
            conn.query_row("SELECT type FROM sqlite_schema WHERE name = ?", [name], OneValue::read).optional()?;
        if let Some(kind) = kind {
            // SQLite's own word for what it is, `table`, `view`, `index` or `trigger`, is the word that drops it.
            conn.execute_batch(&format!("DROP {kind} \"{}\"", name.replace('"', "\"\"")))?;
        }
    }
    Ok(())
}
