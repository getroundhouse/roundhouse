//! What a wait keeps of the pull requests it watches, since the floor.

use rusqlite::{Transaction, params};
use serde_json::Value;

use crate::store::{ByColumn, Columns, StoreError, columns};

columns! {
    /// The columns [`pull_requests_as_facets`] reads of each wait.
    struct WatchColumns { id, watch }
}

/// Each wait's watch keeps the pull requests it last saw as their facets, which an older build's rows can't become,
/// since they lack most of a pull request's summary. So each keeps none, and keeps what it asked for, what it saw as it
/// started, and its deadline: the hub's next read of an open wait's finds them again, and judges it by them, and an
/// ended one's ending says what ended it. Moving an open wait's watch moves its chat's revision, so a client hears it.
pub(super) fn pull_requests_as_facets(tx: &Transaction<'_>) -> Result<(), StoreError> {
    let c = WatchColumns::at(0);
    let rows = tx
        .prepare(&format!("SELECT {} FROM waits WHERE watch IS NOT NULL", WatchColumns::LIST))?
        .query_map([], |row| Ok((row.at::<String>(c.id)?, row.at::<String>(c.watch)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut write = tx.prepare("UPDATE waits SET watch = ?2 WHERE id = ?1")?;
    // What isn't a watch is left as it was, rather than taken for a wait with none, which is one on the workspace's own.
    let rows = rows.into_iter().filter_map(|(id, watch)| Some((id, without_pull_requests(&watch)?)));
    for (id, watch) in rows {
        write.execute(params![id, watch])?;
    }
    Ok(())
}

/// `watch`, as an older build kept it, with none of the pull requests it saw. `None` for what isn't a watch.
fn without_pull_requests(watch: &str) -> Option<String> {
    let mut watch: Value = serde_json::from_str(watch).ok()?;
    *watch.pointer_mut("/snapshot/pull_requests")? = Value::Array(Vec::new());
    Some(watch.to_string())
}
