use super::super::{Hub, off_thread};
use crate::logins::{AccountChange, Connect, Handed, Logins};
use crate::store::now_ms;
use protocol::client::accounts::{AccountConnect, AccountSetDefault};
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{Account, AccountId, AgentKind, HubSettings};
use std::sync::Arc;
use tracing::error;

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

    /// The selected hub login wins over machine credentials on every provider.
    pub(crate) async fn login_for(&self, agent: AgentKind) -> Result<Option<Handed>, ApiError> {
        self.on_logins(move |logins| logins.to_hand(agent, now_ms())).await
    }

    pub(crate) async fn account_set_default(&self, asked: AccountSetDefault) -> Result<HubSettings, ApiError> {
        if self.agent(asked.agent)?.login.is_none() {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "the hub can hold no login for that kind of agent"));
        }
        let settings = self.on_logins(move |logins| logins.set_default(asked.agent, &asked.account, now_ms())).await?;
        self.notify(HubNotice::HubSettings(settings.clone()));
        Ok(settings)
    }

    pub(super) fn announce_account(&self, AccountChange { account, changed, defaults }: AccountChange) -> Account {
        if changed {
            self.notify(HubNotice::Account(account.clone()));
        }
        if let Some(settings) = defaults {
            self.notify(HubNotice::HubSettings(settings));
        }
        account
    }

    pub(super) async fn on_logins<T, F>(&self, f: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&Logins) -> Result<T, ApiError> + Send + 'static,
    {
        let logins = Arc::clone(&self.logins);
        off_thread(move || f(&logins)).await
    }
}
