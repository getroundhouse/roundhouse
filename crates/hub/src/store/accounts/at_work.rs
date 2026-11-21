//! How many chats are at work on each account, which Automatic shares an account's usage among.

use std::collections::HashMap;

use super::super::{ByColumn, Columns, Store, StoreError, columns, lock, text_of};
use protocol::model::{AccountId, SessionStatus};

impl Store {
    /// How many open chats on the hub's login for each account are on a turn now, working or waiting on the person,
    /// by account. One with none at work isn't listed.
    pub(crate) fn at_work_on_accounts(&self) -> Result<HashMap<AccountId, usize>, StoreError> {
        columns! {
            struct AtWork { account_id, chats = "count(*)" }
        }
        let c = AtWork::at(0);
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {} FROM sessions
                 WHERE account_source = 'hub' AND account_id IS NOT NULL AND closed_at_ms IS NULL
                     AND status IN (?, ?)
                 GROUP BY account_id",
            AtWork::LIST
        ))?;
        let rows = statement
            .query_map([text_of(&SessionStatus::Running), text_of(&SessionStatus::WaitingApproval)], |row| {
                Ok((AccountId::from(row.at::<String>(c.account_id)?), row.at::<usize>(c.chats)?))
            })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
