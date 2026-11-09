//! How far each session has been read, which says whether its newest reply is unread.

use protocol::model::{Seq, SessionId};
use rusqlite::{Connection, OptionalExtension, params};

use super::sessions::{SESSION_COLUMNS, session_from_row};
use super::{Store, StoreError, StoredSession, lock};

#[cfg(test)]
mod tests;

/// A session after asking to change how far it's read, and whether that changed anything.
pub struct Reading {
    pub session: StoredSession,
    pub changed: bool,
}

impl Store {
    /// Marks the session read through `through_seq`, but never past its newest event, so a reply that hasn't come
    /// yet can't count as read. It only ever moves forward.
    pub fn mark_read(&self, id: &SessionId, through_seq: Seq) -> Result<Reading, StoreError> {
        let through = i64::try_from(through_seq).unwrap_or(i64::MAX);
        self.change_reading(
            id,
            "UPDATE sessions SET read_seq = MIN(?1, last_seq) WHERE id = ?2 AND read_seq < MIN(?1, last_seq)",
            params![through, id.as_str()],
        )
    }

    /// Marks the newest reply unread again, if there is one and it's been read.
    pub fn mark_unread(&self, id: &SessionId) -> Result<Reading, StoreError> {
        self.change_reading(
            id,
            "UPDATE sessions SET read_seq = reply_seq - 1 WHERE id = ?1 AND reply_seq > 0 AND read_seq >= reply_seq",
            params![id.as_str()],
        )
    }

    /// Runs `update` and reads the session back on the writer, so what comes back is what the update left.
    fn change_reading(
        &self,
        id: &SessionId,
        update: &str,
        params: impl rusqlite::Params,
    ) -> Result<Reading, StoreError> {
        let conn = lock(&self.writer);
        let changed = conn.execute(update, params)? > 0;
        Ok(Reading { session: session_on(&conn, id)?, changed })
    }
}

/// The session as `conn` sees it, which on the writer is what its last change left.
pub(super) fn session_on(conn: &Connection, id: &SessionId) -> Result<StoredSession, StoreError> {
    conn.query_row(
        &format!("SELECT {SESSION_COLUMNS}, rev FROM sessions WHERE id = ?"),
        [id.as_str()],
        session_from_row,
    )
    .optional()?
    .ok_or_else(|| StoreError::NotFound { kind: "session", id: id.to_string() })
}
