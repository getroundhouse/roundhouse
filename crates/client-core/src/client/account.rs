//! The call for whose account a session's agent runs under, as clients made it before accounts were records. Only
//! tests make it now, to show a hub still answers it.

use protocol::client::sessions;
use protocol::model::{SessionAccount, SessionId};

use super::{Client, Result};

impl Client {
    /// See [`sessions::SessionAccount`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn session_account(&self, id: &SessionId) -> Result<SessionAccount> {
        self.call(sessions::SessionAccount { id: id.clone() })
    }
}
