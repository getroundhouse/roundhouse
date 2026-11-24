//! What the store does for what the hub observes of a session, which it keeps as observations, in
//! `crate::hub::observations`, and no longer here.

use std::collections::HashSet;

use protocol::model::SessionId;

use super::super::{OneValue, Store, StoreError, lock};

impl Store {
    /// Every session's ID, so the hub forgets what it observed of one that's gone.
    pub(crate) fn session_ids(&self) -> Result<HashSet<SessionId>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare("SELECT id FROM sessions")?;
        let ids = statement.query_map([], OneValue::read::<String>)?;
        Ok(ids.map(|id| id.map(SessionId::from)).collect::<rusqlite::Result<_>>()?)
    }
}
