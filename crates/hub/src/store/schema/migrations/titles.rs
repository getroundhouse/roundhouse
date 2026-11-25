//! Workspaces' and chats' titles, as they're stored.

use protocol::model::Title;
use rusqlite::{Transaction, params};
use tracing::warn;

use crate::store::{ByColumn, Columns, StoreError, columns};

columns! {
    /// The columns [`tidied`] reads of each workspace and chat.
    struct TitleColumns { rowid, title }
}

/// Each workspace's and chat's title as [`Title::tidied`] leaves it, which is how every build since before the floor
/// writes one, so the hub reads a title as it's stored. An older build wrote some as it found them, such as a chat's
/// first prompt's line, and its reads tidied them. One with nothing left once tidied is NULL, which a record read as no
/// title already. Where it came from stays, so each record reads as it did. Only a title that changes is written, and
/// that moves its record's revision, as any change to it does. What isn't text, which no build wrote, is left as it
/// is, and the log says how many.
pub(super) fn tidied(tx: &Transaction<'_>) -> Result<(), StoreError> {
    for table in ["workspaces", "sessions"] {
        let left = tidy(tx, table)?;
        if left > 0 {
            warn!(
                name: "database.titles_left",
                table,
                left,
                "Left titles that aren't text as they were, which no record can read"
            );
        }
    }
    Ok(())
}

/// Tidies every title in `table`, and returns how many it left since they aren't text.
fn tidy(tx: &Transaction<'_>, table: &str) -> Result<usize, StoreError> {
    let c = TitleColumns::at(0);
    let (mut changed, mut left) = (Vec::new(), 0);
    let mut read = tx.prepare(&format!("SELECT {} FROM {table} WHERE title IS NOT NULL", TitleColumns::LIST))?;
    let mut rows = read.query([])?;
    while let Some(row) = rows.next()? {
        // What isn't text is left, and so is text that isn't UTF-8.
        let Ok(stored) = row.borrowed_at(c.title)?.as_str() else {
            left += 1;
            continue;
        };
        let title = Title::tidied(stored);
        if title.as_deref() != Some(stored) {
            changed.push((row.at::<i64>(c.rowid)?, title));
        }
    }
    drop(rows);
    drop(read);
    let mut write = tx.prepare(&format!("UPDATE {table} SET title = ?2 WHERE rowid = ?1"))?;
    for (row, title) in changed {
        write.execute(params![row, title.as_deref()])?;
    }
    Ok(left)
}
