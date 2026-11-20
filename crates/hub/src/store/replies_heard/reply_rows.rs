use super::super::{Store, StoreError, current_rev, lock, rev_of};
use protocol::model::{Ending, MessageId, ReplyHeard, Rev, SessionId};
use rusqlite::{OptionalExtension, Row, params};

const REPLY_HEARD_COLUMNS: &str = "session_id, message, ending, rev";

impl Store {
    /// Keeps how the reply `message` ended, unless how it ended is kept already, since the first that's said of a
    /// reply is kept. Returns the record if it's new.
    pub fn keep_reply_heard(
        &self,
        session_id: &SessionId,
        message: &MessageId,
        ending: Ending,
    ) -> Result<Option<ReplyHeard>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let kept = tx.execute(
            "INSERT INTO replies_heard (session_id, message, ending) VALUES (?, ?, ?) ON CONFLICT DO NOTHING",
            params![session_id.as_str(), message.as_str(), serde_json::to_string(&ending)?],
        )?;
        let heard = if kept == 0 {
            None
        } else {
            tx.query_row(
                &format!("SELECT {REPLY_HEARD_COLUMNS} FROM replies_heard WHERE session_id = ? AND message = ?"),
                params![session_id.as_str(), message.as_str()],
                reply_heard_from_row,
            )
            .optional()?
        };
        tx.commit()?;
        Ok(heard)
    }

    /// How each of the session's replies said out loud ended, read in one transaction with the revision they're as of.
    pub fn list_replies_heard(&self, session_id: &SessionId) -> Result<(Vec<ReplyHeard>, Rev), StoreError> {
        let mut conn = lock(&self.reader);
        let tx = conn.transaction()?;
        let heard = tx
            .prepare_cached(&format!(
                "SELECT {REPLY_HEARD_COLUMNS} FROM replies_heard WHERE session_id = ? ORDER BY rev"
            ))?
            .query_map([session_id.as_str()], reply_heard_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let rev = current_rev(&tx)?;
        tx.commit()?;
        Ok((heard, rev))
    }
}

/// Written only by this hub, as JSON. An ending it can't read is one it doesn't know.
fn reply_heard_from_row(row: &Row<'_>) -> rusqlite::Result<ReplyHeard> {
    Ok(ReplyHeard {
        session_id: SessionId::from(row.get::<_, String>(0)?),
        message: MessageId::from(row.get::<_, String>(1)?),
        ending: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or(Ending::Unknown),
        rev: rev_of(row, 3)?,
    })
}
