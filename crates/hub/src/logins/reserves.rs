//! What each account reserves of its week, set under the logins' lock, so it never races a login being connected to
//! an account or disconnected from it.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{AccountId, Reserve};

use super::{AccountChange, Logins};

impl Logins {
    /// Sets what `id` reserves of its week, as `AccountSetReserve` says. Refused for an account with no record, and for
    /// one whose login the hub didn't add, since only those are put on automatically.
    pub(crate) fn set_reserve(&self, id: &AccountId, reserve: Option<Reserve>) -> Result<AccountChange, ApiError> {
        let _files = self.files.lock();
        let account = self.record(id)?;
        if account.login_agent.is_none() {
            let message = "only an account whose login was added to the hub can reserve part of its week";
            return Err(ApiError::new(ErrorCode::InvalidRequest, message));
        }
        Ok(match self.store.set_reserve(id, reserve)? {
            Some(account) => AccountChange { account, changed: true, defaults: None },
            None => AccountChange { account, changed: false, defaults: None },
        })
    }
}
