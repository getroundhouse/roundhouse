//! Connecting a login for agents' accounts to run under, and disconnecting one, as a client asks, and which login an
//! agent is launched under. What's kept, and in what order, is `crate::logins`'s.

use std::sync::Arc;

use protocol::client::accounts::AccountConnect;
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{Account, AccountId, AgentKind};
use protocol::worker::AccountFor;
use tracing::{error, warn};

use super::{Hub, off_thread};
use crate::logins::{AccountChange, Connect, Handed, Logins};
use crate::store::now_ms;
use crate::worker_link::WorkerLink;

#[cfg(test)]
mod tests;

impl Hub {
    /// Has the hub hold a login, as `AccountConnect` says, and tells every client what that changed of the account's
    /// record.
    pub(crate) async fn account_connect(&self, asked: AccountConnect) -> Result<Account, ApiError> {
        let Some(login) = &self.agent(asked.agent)?.login else {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "the hub can hold no login for that kind of agent"));
        };
        let connect = Connect::new(asked, login, now_ms())?;
        let change = self.on_logins(move |logins| logins.connect(&connect)).await?;
        Ok(self.announce_account(change))
    }

    /// Has the hub forget the login it holds for the account, as `AccountDisconnect` says, and tells every client when
    /// that changed the account's record.
    pub(crate) async fn account_disconnect(&self, id: AccountId) -> Result<Account, ApiError> {
        let change = self.on_logins(move |logins| logins.disconnect(&id)).await?;
        Ok(self.announce_account(change))
    }

    /// Makes the logins' files and the accounts' rows agree, as the hub starts. If it fails, they stay as they are
    /// until the next start, which costs little: a file no row names is never read, and a launch that would be handed a
    /// login whose token is gone fails, saying so. Not starting the hub over it would cost every workspace.
    pub(crate) async fn tidy_logins(&self) {
        if let Err(e) = self.on_logins(Logins::tidy).await {
            error!("couldn't make the logins' files and the accounts agree: {}", e.message);
        }
    }

    /// The login an agent of `agent`'s kind runs under on `link`'s machine, launched or for a one-off call, with the
    /// account the hub holds it for. `None` when the machine has a login of its own for that kind, as its worker reads
    /// it just before, on any provider, so a Mac the person is logged in on keeps their own, and a server they never
    /// logged in on gets the hub's. `None` too when the hub holds none for that kind that hasn't ended, and then the
    /// worker isn't asked. Either way the agent then runs under its machine's own login, if it has one. The token is
    /// read only once the machine has said it has none, so a launch that keeps its machine's own never needs it.
    ///
    /// A worker that can't say, as when the CLI times out or answers in a way the adapter can't read, gets the hub's
    /// login. That launch signs in whatever the machine has, where one given none signs in only if the machine has a
    /// login, and the hub's record of the launch then says truly which login the agent got.
    pub(crate) async fn login_for(&self, link: &WorkerLink, agent: AgentKind) -> Result<Option<Handed>, ApiError> {
        let now = now_ms();
        if !self.on_logins(move |logins| logins.holds(agent, now)).await? {
            return Ok(None);
        }
        match link.call(AccountFor { agent, login: None }).await {
            Ok(own) if own.logged_in => return Ok(None),
            Ok(_) => {}
            Err(e) => {
                warn!(?agent, "couldn't tell whether the machine has a login of its own, so it gets the hub's: {e}");
            }
        }
        self.on_logins(move |logins| logins.to_hand(agent, now)).await
    }

    fn announce_account(&self, AccountChange { account, changed }: AccountChange) -> Account {
        if changed {
            self.notify(HubNotice::Account(account.clone()));
        }
        account
    }

    async fn on_logins<T, F>(&self, f: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&Logins) -> Result<T, ApiError> + Send + 'static,
    {
        let logins = Arc::clone(&self.logins);
        off_thread(move || f(&logins)).await
    }
}
