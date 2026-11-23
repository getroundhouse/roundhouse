//! What the hub keeps of each repository, since the floor.

use protocol::model::{RepoLink, RepoUrl};
use rusqlite::{Transaction, params};

use crate::store::{ByColumn, Columns, StoreError, columns};

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
