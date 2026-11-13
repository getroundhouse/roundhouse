//! Which login a launch runs under: the hub's for the account a chat is on, none for a chat on the ambient login, or
//! else its kind's default, for a chat on none yet. All of it is durable hub state, never inferred from the worker's
//! machine or renewal order.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{AccountId, AgentKind, AgentLogin, HubSettings, LoginToken};

use super::{Handed, Logins};
use crate::error::internal;
use crate::login_files::Locked;
use crate::store::{Held, SessionLogin};

/// The account a chat's launches run under, and why, since what the person can do about one that can't be used depends
/// on it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    /// The account the session is on, as the hub's login for it.
    Session(AccountId),
    /// Its kind's default, for a chat not on an account yet, which the launch puts it on.
    Default(AccountId),
}

impl Target {
    fn account(&self) -> &AccountId {
        match self {
            Target::Session(account) | Target::Default(account) => account,
        }
    }

    /// What's wrong with it once its login can't be used, called `called`, and what the person can do.
    fn needs_a_login(&self, called: &str) -> String {
        match self {
            Target::Session(_) => format!(
                "this chat's account {called} needs a login; renew or reconnect it in Accounts, or move the chat to \
                 another account"
            ),
            Target::Default(_) => format!(
                "the default account {called} needs a login; renew or reconnect it in Accounts, or choose another \
                 default"
            ),
        }
    }
}

/// What's wrong with an account the person is choosing, for a default or a chat, once its login can't be used. It
/// isn't the default or the chat's yet, so it's named alone.
fn needs_a_login_first(called: &str) -> String {
    format!("{called} needs a login; renew or reconnect it in Accounts first")
}

impl Logins {
    pub(crate) fn set_default(
        &self,
        agent: AgentKind,
        account: &AccountId,
        now_ms: i64,
    ) -> Result<HubSettings, ApiError> {
        let files = self.files.lock();
        self.choosable(&files, agent, account, now_ms)?;
        Ok(self.store.set_account_default(agent, account)?)
    }

    /// Refuses moving a chat of `agent`'s kind to `account` as choosing it as a default would be refused: an account with
    /// no record, one that isn't a login added for that kind, and one whose login can't be used now, since a move to it
    /// would leave the chat unable to launch.
    pub(crate) fn can_move_to(&self, agent: AgentKind, account: &AccountId, now_ms: i64) -> Result<(), ApiError> {
        let files = self.files.lock();
        self.choosable(&files, agent, account, now_ms).map(drop)
    }

    /// The login a launch of a chat of `agent`'s kind that's `on` a login is handed: the hub's for the account it's on,
    /// or else, for a chat on none yet, its kind's default. One whose login can't be used fails closed, whichever it
    /// is. A chat on the ambient login is handed none, whatever the default is, and so is one on none of a kind with no
    /// default, which goes on the ambient login once its worker says whose that is.
    pub(crate) fn to_hand(&self, agent: AgentKind, on: &SessionLogin, now_ms: i64) -> Result<Option<Handed>, ApiError> {
        let files = self.files.lock();
        let Some(target) = self.target(agent, on)? else { return Ok(None) };
        self.available(&files, agent, target.account(), now_ms, |called| target.needs_a_login(called)).map(Some)
    }

    /// The account whose login a chat of `agent`'s kind that's `on` a login runs under: the one it's on, or else the
    /// kind's default. `None` for the ambient login.
    fn target(&self, agent: AgentKind, on: &SessionLogin) -> Result<Option<Target>, ApiError> {
        match on {
            SessionLogin::Hub(account) => return Ok(Some(Target::Session(account.clone()))),
            SessionLogin::Ambient => return Ok(None),
            SessionLogin::NoneYet => {}
        }
        let settings = self.store.hub_settings()?;
        let default = settings.account_defaults.unwrap_or_default().into_iter().find(|choice| choice.agent == agent);
        Ok(default.map(|choice| Target::Default(choice.account)))
    }

    /// Whether the person can choose `account` for `agent`'s kind, as a default or a chat's: a login added for that
    /// kind, which can be used now.
    fn choosable(
        &self,
        files: &Locked<'_, LoginToken>,
        agent: AgentKind,
        account: &AccountId,
        now_ms: i64,
    ) -> Result<Handed, ApiError> {
        self.renewable(account, agent)?;
        self.available(files, agent, account, now_ms, needs_a_login_first)
    }

    /// The login the hub holds for `account`, if it can be used now. Refused with `Conflict` and what
    /// `needs_a_login` says of the account, by what it's called, when it can't.
    fn available(
        &self,
        files: &Locked<'_, LoginToken>,
        agent: AgentKind,
        account: &AccountId,
        now_ms: i64,
        needs_a_login: impl Fn(&str) -> String,
    ) -> Result<Handed, ApiError> {
        let record = self.record(account)?;
        let called = record.name.as_deref().unwrap_or(account.as_str());
        let unavailable = || ApiError::new(ErrorCode::Conflict, needs_a_login(called));
        let held = self.store.held_logins(agent)?.into_iter().find(|held| held.account == *account);
        let Some(Held { file, login, .. }) = held else { return Err(unavailable()) };
        if !login.works_at(now_ms) {
            return Err(unavailable());
        }
        let token = files
            .read(&file)
            .map_err(|e| internal(format!("couldn't read {called}'s login: {e}")))?
            .ok_or_else(unavailable)?;
        Ok(Handed { account: account.clone(), login: AgentLogin { token } })
    }
}
