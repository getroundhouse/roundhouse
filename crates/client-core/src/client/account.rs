//! The calls about the accounts agents run under: connecting a login the hub holds for one, disconnecting it, and
//! whose account a session's agent runs under, as clients asked before accounts were records, which only tests ask now,
//! to show a hub still answers it, so only the `testing` feature builds it.

use protocol::client::accounts;
#[cfg(feature = "testing")]
use protocol::client::sessions;
use protocol::model::{Account, AccountId, AgentKind, HubSettings, LoginToken};
#[cfg(feature = "testing")]
use protocol::model::{SessionAccount, SessionId};

use super::{Client, Result};

impl Client {
    /// Chooses the hub's default login for new processes of this agent kind.
    pub fn account_set_default(&self, agent: AgentKind, account: &AccountId) -> Result<HubSettings> {
        self.call(accounts::AccountSetDefault { agent, account: account.clone() })
    }

    /// Gives the hub a login for agents of `agent`'s kind, as a new account called `name`, or in place of the login of
    /// `account`, as renewing it does, and answers with the account's record. See [`accounts::AccountConnect`]. A hub
    /// older than this refuses it, as `is_unsupported` says.
    pub fn account_connect(
        &self,
        agent: AgentKind,
        name: &str,
        token: LoginToken,
        account: Option<AccountId>,
    ) -> Result<Account> {
        self.call(accounts::AccountConnect { agent, name: name.to_string(), token, account })
    }

    /// Has the hub forget the login it holds for `id`, and answers with the account's record, which stays. See
    /// [`accounts::AccountDisconnect`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn account_disconnect(&self, id: &AccountId) -> Result<Account> {
        self.call(accounts::AccountDisconnect { id: id.clone() })
    }

    /// See [`sessions::SessionAccount`]. A hub older than this refuses it, as `is_unsupported` says.
    #[cfg(feature = "testing")]
    pub fn session_account(&self, id: &SessionId) -> Result<SessionAccount> {
        self.call(sessions::SessionAccount { id: id.clone() })
    }
}
