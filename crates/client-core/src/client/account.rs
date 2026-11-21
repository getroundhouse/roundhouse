//! The calls about the accounts agents run under: connecting a login the hub holds for one, disconnecting it, choosing
//! a kind's default, reserving part of one's week, and moving a chat to another. Asking whose account a session's
//! agent runs under, as clients did before accounts were records, is left only to tests, to show a hub still answers
//! it, so only the `testing` feature builds that one.

use protocol::client::{accounts, sessions};
#[cfg(any(test, feature = "testing"))]
use protocol::model::SessionAccount;
use protocol::model::{Account, AccountId, AgentKind, HubSettings, LoginToken, Reserve, Session, SessionId};

use super::{Client, Result};

impl Client {
    /// Chooses the account chats of this agent kind are put on while they're on none. See
    /// [`accounts::AccountSetDefault`].
    pub fn account_set_default(&self, agent: AgentKind, account: &AccountId) -> Result<HubSettings> {
        self.call(accounts::AccountSetDefault { agent, account: account.clone() })
    }

    /// Puts chats of this agent kind on accounts automatically, by how much of its week each has to spare. See
    /// [`accounts::AccountSetAutomatic`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn account_set_automatic(&self, agent: AgentKind) -> Result<HubSettings> {
        self.call(accounts::AccountSetAutomatic { agent })
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

    /// Sets how much of `account`'s week the person keeps for themselves, or none, and answers with its record. See
    /// [`accounts::AccountSetReserve`]. A hub older than this refuses it, as `is_unsupported` says.
    pub fn account_set_reserve(&self, account: &AccountId, reserve: Option<Reserve>) -> Result<Account> {
        self.call(accounts::AccountSetReserve { account: account.clone(), reserve })
    }

    /// Moves the chat `id` to `account`, and answers with its record. With `force`, an agent that counts as running
    /// under another account is stopped rather than refused with `NeedsForce`. See [`sessions::SessionAssignAccount`].
    /// A hub older than this refuses it, as `is_unsupported` says.
    pub fn session_assign_account(&self, id: &SessionId, account: &AccountId, force: bool) -> Result<Session> {
        self.call(sessions::SessionAssignAccount { id: id.clone(), account: account.clone(), force })
    }

    /// See [`sessions::SessionAccount`]. A hub older than this refuses it, as `is_unsupported` says.
    #[cfg(any(test, feature = "testing"))]
    pub fn session_account(&self, id: &SessionId) -> Result<SessionAccount> {
        self.call(sessions::SessionAccount { id: id.clone() })
    }
}
