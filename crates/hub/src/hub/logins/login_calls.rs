use super::super::{Hub, off_thread};
use crate::hub::observations::AccountLimits;
use crate::logins::{AccountChange, Connect, Handed, Launching, Logins};
use crate::store::{SessionLogin, now_ms};
use protocol::client::accounts::{AccountConnect, AccountSetAutomatic, AccountSetDefault, AccountSetReserve};
use protocol::client::sessions::SessionAssignAccount;
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{Account, AccountId, AgentKind, HubSettings, LoginDescription, Session};
use std::sync::Arc;
use tracing::error;

impl Hub {
    /// Has the hub hold a login, as `AccountConnect` says, and tells every client what that changed of the account's
    /// record. The kind's lists of models are let go of, since the login can be another plan's, or the kind's default.
    pub(crate) async fn account_connect(&self, asked: AccountConnect) -> Result<Account, ApiError> {
        let (login, agent) = (self.login_of(asked.agent)?, asked.agent);
        let connect = Connect::new(asked, &login, now_ms())?;
        let change = self.on_logins(move |logins| logins.connect(&connect)).await?;
        self.forget_model_lists(|key| key.agent == agent).await;
        Ok(self.announce_account(change))
    }

    /// Has the hub forget the login it holds for the account, as `AccountDisconnect` says, and tells every client when
    /// that changed the account's record. The lists of models under its login are let go of.
    pub(crate) async fn account_disconnect(&self, id: AccountId) -> Result<Account, ApiError> {
        let gone = id.clone();
        let change = self.on_logins(move |logins| logins.disconnect(&id)).await?;
        self.forget_model_lists(|key| key.account.as_ref() == Some(&gone)).await;
        Ok(self.announce_account(change))
    }

    /// Sets how much of the account's week the person keeps for themselves, as `AccountSetReserve` says, and tells
    /// every client when that changed the account's record.
    pub(crate) async fn account_set_reserve(&self, asked: AccountSetReserve) -> Result<Account, ApiError> {
        let AccountSetReserve { account, reserve } = asked;
        let change = self.on_logins(move |logins| logins.set_reserve(&account, reserve)).await?;
        Ok(self.announce_account(change))
    }

    /// What makes a login for `agent`'s kind, as its adapter says, which only a kind the hub can hold a login for has.
    /// Refused with `InvalidRequest` for any other, since nothing about a held login can be asked of it.
    fn login_of(&self, agent: AgentKind) -> Result<LoginDescription, ApiError> {
        let refused = || ApiError::new(ErrorCode::InvalidRequest, "the hub can hold no login for that kind of agent");
        self.agent(agent)?.login.clone().ok_or_else(refused)
    }

    /// Makes the logins' files and the accounts' rows agree, as the hub starts, agents', then GitHub's, and then the
    /// tool servers'. If it fails, they stay as they are until the next start, which costs little: a file no row names is never read, and a launch
    /// that would be handed a login whose token is gone fails, saying so. Not starting the hub over it would cost every
    /// workspace.
    pub(crate) async fn tidy_logins(&self) {
        if let Err(e) = self.on_logins(Logins::tidy).await {
            error!(
                name: "login.files_disagree",
                error = e.message,
                "Couldn't make the logins' files and the accounts agree"
            );
        }
        self.tidy_github_logins().await;
        self.tidy_tool_servers().await;
    }

    /// The login a launch of a chat of `agent`'s kind that's `on` a login runs under, as [`Logins::to_launch`] says.
    pub(crate) async fn launching(&self, agent: AgentKind, on: SessionLogin) -> Result<Launching, ApiError> {
        let heard = self.observations.values_of::<AccountLimits>();
        self.on_logins(move |logins| logins.to_launch(agent, &on, &heard, now_ms())).await
    }

    /// The login a launch of a chat of `agent`'s kind that's `on` a login is handed, as [`Logins::to_hand`] says: the
    /// hub's, for the account it's on or its kind's default, which wins over ambient credentials on every provider, or
    /// none, for a chat on the ambient login.
    pub(crate) async fn login_for(&self, agent: AgentKind, on: SessionLogin) -> Result<Option<Handed>, ApiError> {
        let heard = self.observations.values_of::<AccountLimits>();
        self.on_logins(move |logins| logins.to_hand(agent, &on, &heard, now_ms())).await
    }

    /// Moves a chat to another account, as `SessionAssignAccount` says. Not through the session's queue, which a prompt
    /// waiting on setup holds, as `session_configure` says.
    pub(crate) async fn session_assign_account(
        self: &Arc<Self>,
        asked: SessionAssignAccount,
    ) -> Result<Session, ApiError> {
        let SessionAssignAccount { id, account, force } = asked;
        self.live_session(&id).await?.assign_account(account, force).await
    }

    /// Refuses moving a chat of `agent`'s kind to `account` as choosing it as a default is refused, as
    /// [`Logins::can_move_to`] says.
    pub(crate) async fn can_move_to(&self, agent: AgentKind, account: AccountId) -> Result<(), ApiError> {
        self.login_of(agent)?;
        self.on_logins(move |logins| logins.can_move_to(agent, &account, now_ms())).await
    }

    pub(crate) async fn account_set_default(&self, asked: AccountSetDefault) -> Result<HubSettings, ApiError> {
        self.login_of(asked.agent)?;
        let agent = asked.agent;
        let settings = self.on_logins(move |logins| logins.set_default(asked.agent, &asked.account, now_ms())).await?;
        // A launch that names no account is handed another login now, so its lists of models no longer apply.
        self.forget_model_lists(|key| key.agent == agent).await;
        self.notify(HubNotice::HubSettings(settings.clone()));
        Ok(settings)
    }

    /// Puts the kind's chats on accounts automatically, as `AccountSetAutomatic` says, and tells every client.
    pub(crate) async fn account_set_automatic(&self, asked: AccountSetAutomatic) -> Result<HubSettings, ApiError> {
        let agent = asked.agent;
        self.login_of(agent)?;
        let settings = self.on_logins(move |logins| logins.set_automatic(agent)).await?;
        // A launch that names no account may be handed another login now, so its lists of models no longer apply.
        self.forget_model_lists(|key| key.agent == agent).await;
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
