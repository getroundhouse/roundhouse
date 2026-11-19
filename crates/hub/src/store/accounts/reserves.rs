//! How much of each account's week the person keeps for themselves, which Automatic keeps out of when it puts a chat on
//! an account.

use super::super::{Store, StoreError, lock};
use super::account_or_missing;
use protocol::model::{Account, AccountId, Reserve};

impl Store {
    /// Sets how much of `account`'s week is reserved, or none, and returns its record when that changed it, with a newer
    /// revision. One that reserves that already is left as it is, so its revision doesn't move.
    pub fn set_reserve(&self, account: &AccountId, reserve: Option<Reserve>) -> Result<Option<Account>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let percent = reserve.map(Reserve::percent);
        let changed = tx.execute(
            "UPDATE accounts SET reserve_percent = ?2 WHERE id = ?1 AND reserve_percent IS NOT ?2",
            rusqlite::params![account.as_str(), percent],
        )? > 0;
        let record = account_or_missing(&tx, account)?;
        tx.commit()?;
        Ok(changed.then_some(record))
    }
}
