//! What the hub keeps of each repository, since the floor.

use protocol::model::{RepoLink, RepoUrl};
use rusqlite::{Transaction, params};

use crate::store::{ByColumn, Columns, OneValue, StoreError, columns};

columns! {
    /// The columns [`links`] reads of each repository.
    struct AddressColumns { id, url }
}

/// Each repository's link to its repository on a host, as JSON, and why one whose address is on github.com isn't
/// linked, as JSON, both moving its revision as they change. A repository whose address a worker read before is linked
/// by it at once where it isn't on github.com, as every such address is, and one on github.com is left to the hub,
/// which asks GitHub which repository it names as it starts. Linking one moves its revision, so a client hears it.
pub(super) fn links(tx: &Transaction<'_>) -> Result<(), StoreError> {
    tx.execute_batch(
        "ALTER TABLE repos ADD COLUMN link TEXT;
         ALTER TABLE repos ADD COLUMN unlinked TEXT;
         DROP TRIGGER repos_rev_on_update;
         CREATE TRIGGER repos_rev_on_update AFTER UPDATE OF name, path, url, link, unlinked, created_at_ms ON repos
             WHEN NEW.rev = OLD.rev BEGIN
             UPDATE counters SET value = value + 1 WHERE name = 'rev';
             UPDATE repos SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
         END;",
    )?;
    let c = AddressColumns::at(0);
    let rows = tx
        .prepare(&format!("SELECT {} FROM repos WHERE url IS NOT NULL", AddressColumns::LIST))?
        .query_map([], |row| Ok((row.at::<String>(c.id)?, row.at::<String>(c.url)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut write = tx.prepare("UPDATE repos SET link = ?2 WHERE id = ?1")?;
    // An address that can't be one, which only a database changed by hand holds, is left for the hub to refuse as
    // it reads the row.
    let plain = rows.into_iter().filter_map(|(id, url)| Some((id, RepoUrl::try_from(url).ok()?)));
    for (id, url) in plain.filter(|(_, url)| url.on_github().is_none()) {
        write.execute(params![id, serde_json::to_string(&RepoLink::Plain { url })?])?;
    }
    Ok(())
}

/// A repository's checkout may be none, for one added by its link alone, which a workspace on any machine clones, so
/// such a row has an address. Checkouts are still each one repository's.
///
/// SQLite can't drop a `NOT NULL` with `ALTER TABLE`, and rebuilding the table under a transaction would leave every
/// workspace's reference to it broken, since foreign keys are on. So this edits the table's definition in place, as
/// SQLite's own procedure for dropping a `NOT NULL` does (<https://www.sqlite.org/lang_altertable.html#otheralter>),
/// which leaves every row as it is, and raises the schema's version so every connection reads it again.
pub(super) fn without_checkouts(tx: &Transaction<'_>) -> Result<(), StoreError> {
    let version: i64 = tx.pragma_query_value(None, "schema_version", OneValue::read)?;
    tx.pragma_update(None, "writable_schema", true)?;
    let edited = tx.execute(
        "UPDATE sqlite_schema SET sql = replace(sql, 'path TEXT NOT NULL UNIQUE', 'path TEXT UNIQUE')
             WHERE type = 'table' AND name = 'repos' AND sql LIKE '%path TEXT NOT NULL UNIQUE%'",
        [],
    )?;
    tx.pragma_update(None, "writable_schema", false)?;
    if edited != 1 {
        // Only a database changed by hand has another, which is left as it is.
        return Err(rusqlite::Error::InvalidQuery.into());
    }
    tx.pragma_update(None, "schema_version", version + 1)?;
    Ok(())
}
