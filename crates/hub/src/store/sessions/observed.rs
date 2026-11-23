//! What the store does for what the hub observes of a session, which it keeps as observations, in
//! `crate::hub::observations`, and no longer here.

use std::collections::HashSet;

use protocol::model::{Session, SessionId};
use rusqlite::{OptionalExtension as _, TransactionBehavior};

use super::super::{Columns, OneValue, Store, StoreError, lock};
use super::session_rows::{SessionColumns, session_from_row};

impl Store {
    /// Every session's ID, so the hub forgets what it observed of one that's gone.
    pub(crate) fn session_ids(&self) -> Result<HashSet<SessionId>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare("SELECT id FROM sessions")?;
        let ids = statement.query_map([], OneValue::read::<String>)?;
        Ok(ids.map(|id| id.map(SessionId::from)).collect::<rusqlite::Result<_>>()?)
    }

    /// Moves the revision of `id`'s record, since an observation it holds the old form of changed, and returns the
    /// record. `None` for a session that's gone. One transaction, so the record read back has the revision it moved to.
    pub(in super::super) fn session_moved(&self, id: &SessionId) -> Result<Option<Session>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // As the triggers that move a session's revision do, since no column of its row changes.
        tx.execute("UPDATE counters SET value = value + 1 WHERE name = 'rev'", [])?;
        tx.execute(
            "UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE id = ?",
            [id.as_str()],
        )?;
        let session = tx
            .query_row(
                &format!("SELECT {} FROM sessions WHERE id = ?", SessionColumns::LIST),
                [id.as_str()],
                session_from_row,
            )
            .optional()?;
        tx.commit()?;
        Ok(session.map(|stored| stored.session))
    }
}
