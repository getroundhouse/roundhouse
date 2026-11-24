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

/// What each open wait last read, which the hub keeps as an observation, in its cache file, and no longer in the
/// wait's watch, which keeps what it asked for, what it saw as it started, the repository it's pinned to, its deadline,
/// and, once it has ended, its last read. Nothing is carried: an open wait's next read finds its pull requests again,
/// and until then it watches those it was asked for. An ended wait's is left, as it ended. The trigger that moved a
/// chat's revision on a change to an open wait's watch goes, since nothing it shows changes there now.
pub(super) fn reads_observed(tx: &Transaction<'_>) -> Result<(), StoreError> {
    tx.execute_batch("DROP TRIGGER waits_move_session_on_observation;")?;
    let c = WatchColumns::at(0);
    let rows = tx
        .prepare(&format!("SELECT {} FROM waits WHERE watch IS NOT NULL AND ended_at_ms IS NULL", WatchColumns::LIST))?
        .query_map([], |row| Ok((row.at::<String>(c.id)?, row.at::<String>(c.watch)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut write = tx.prepare("UPDATE waits SET watch = ?2 WHERE id = ?1")?;
    let rows = rows.into_iter().filter_map(|(id, watch)| Some((id, without_read(&watch)?)));
    for (id, watch) in rows {
        write.execute(params![id, watch])?;
    }
    Ok(())
}

/// `watch` with none of what it read: no pull requests, no failure and nothing it ended on. `None` for what isn't a
/// watch, which is left as it was.
fn without_read(watch: &str) -> Option<String> {
    let mut watch: Value = serde_json::from_str(watch).ok()?;
    *watch.pointer_mut("/snapshot/pull_requests")? = Value::Array(Vec::new());
    let fields = watch.as_object_mut()?;
    fields.insert("read_error".into(), Value::Null);
    fields.insert("affected".into(), Value::Array(Vec::new()));
    Some(watch.to_string())
}

/// When a wait's pull request last left the merge queue, and last had changes asked for, as the wait saw them as it
/// started, which a wait of the tool that waited on one pull request kept, and which nothing has read or written since
/// that tool went, before the floor. Nothing is carried, since no wait reads them.
pub(super) const SEEN_DROPPED: &str = r"
    ALTER TABLE waits DROP COLUMN seen_left_queue_at_ms;
    ALTER TABLE waits DROP COLUMN seen_changes_requested_at_ms;
";

/// Each wait on a workspace's own pull request that the singular `wait_for_pull_request` tool started and that's still
/// open, ended as called off, now, since nothing judges one. That tool went before the floor, but the two builds before
/// it left a database at the floor's schema with such waits in it. Ending one moves its chat's revision, as any wait's
/// end does, and an agent's CLI asking after one hears it was called off.
pub(super) const UNWATCHED_CALLED_OFF: &str = r"
    UPDATE waits
        SET ended_at_ms = CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER), ended = 'cancelled'
        WHERE watch IS NULL AND ended_at_ms IS NULL;
";
