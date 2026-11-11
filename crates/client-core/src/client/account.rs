//! The call for whose account a session's agent runs under, as clients made it before accounts were records. Only
//! tests make it now, to show a hub still answers it.

use protocol::client::{ClientRequest, ClientResponse};
use protocol::model::{SessionAccount, SessionId};

use super::{Client, Result, unexpected};

impl Client {
    /// See [`ClientRequest::SessionAccount`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn session_account(&self, id: &SessionId) -> Result<SessionAccount> {
        let reply = self.call(ClientRequest::SessionAccount { id: id.clone() })?;
        let ClientResponse::SessionAccount(account) = reply else {
            return unexpected(&reply);
        };
        Ok(account)
    }
}
