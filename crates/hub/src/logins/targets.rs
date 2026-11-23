//! Which login a launch runs under: the hub's for the account a chat is on, none for a chat on the ambient login, or
//! else its kind's default, for a chat on none yet. All of it is durable hub state, never inferred from the worker's
//! machine or renewal order.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{AccountId, AgentKind, AgentLogin, HubSettings, LoginToken};

use super::automatic::{self, Candidate, Heard};
use super::{Handed, Logins};
use crate::error::internal;
use crate::login_files::Locked;
use crate::store::{Held, Placed, SessionLogin};

/// The account a chat's launches run under, and why, since what the person can do about one that can't be used depends
/// on it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    /// The account the session is on, as the hub's login for it.
    Session(AccountId),
    /// Its kind's default, for a chat not on an account yet, which the launch puts it on.
    Default(AccountId),
    /// The account Automatic chose, for a chat not on an account yet, which the launch puts it on.
    Automatic(AccountId),
}

impl Target {
    fn account(&self) -> &AccountId {
        match self {
            Target::Session(account) | Target::Default(account) | Target::Automatic(account) => account,
        }
    }

    /// What's wrong with it once its login can't be used, called `called`, and what the person can do.
    fn needs_a_login(&self, called: &str) -> String {
        match self {
            Target::Session(_) => format!(
                "this chat's account {called} needs a login; renew or reconnect it in Agents, or move the chat to \
                 another account"
            ),
            Target::Default(_) => format!(
                "the default account {called} needs a login; renew or reconnect it in Agents, or choose another \
                 default"
            ),
            Target::Automatic(_) => format!(
                "{called}, which Automatic chose, needs a login; renew or reconnect it in Agents, or disconnect it"
            ),
        }
    }
}

/// Why a chat on none can't start while its kind's chats are put on accounts automatically and no login added for the
/// kind works, so there's no account to put it on whatever's left of any.
const NO_LOGIN_WORKS: &str =
    "no account added for this kind has a login that works; renew or reconnect one in Agents, or choose an account";

/// The login a launch runs under, as the hub's records say.
#[derive(Debug)]
pub(crate) enum Launching {
    /// The hub's login for an account.
    Handed(Handed),
    /// Its machine's own, for a chat on the ambient login, or on none of a kind with no default.
    Ambient,
    /// None, since the chat is on none, its kind's chats are put on accounts automatically, and none has usage to
    /// spare.
    NoAccountToSpare,
}

/// Where a launch is aimed, before its login is checked.
enum Aim {
    At(Target),
    Ambient,
    NoAccountToSpare,
}

/// Why a chat on none can't start while its kind's chats are put on accounts automatically and none has usage to spare.
pub(crate) const NO_ACCOUNT_TO_SPARE: &str =
    "no account has usage to spare: each is down to what it reserves of its week, or has used all of a limit";

/// What's wrong with an account the person is choosing, for a default or a chat, once its login can't be used. It
/// isn't the default or the chat's yet, so it's named alone.
fn needs_a_login_first(called: &str) -> String {
    format!("{called} needs a login; renew or reconnect it in Agents first")
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

    /// Puts `agent`'s chats on accounts automatically, as `AccountSetAutomatic` says. Refused with `Conflict` before a
    /// login has been added for the kind, since there's no account to put them on.
    pub(crate) fn set_automatic(&self, agent: AgentKind) -> Result<HubSettings, ApiError> {
        let _files = self.files.lock();
        self.store.set_automatic(agent)?.ok_or_else(|| {
            ApiError::new(
                ErrorCode::Conflict,
                "add a login for this kind of agent before putting chats on one automatically",
            )
        })
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
    pub(crate) fn to_hand(
        &self,
        agent: AgentKind,
        on: &SessionLogin,
        heard: &Heard,
        now_ms: i64,
    ) -> Result<Option<Handed>, ApiError> {
        match self.to_launch(agent, on, heard, now_ms)? {
            Launching::Handed(handed) => Ok(Some(handed)),
            Launching::Ambient => Ok(None),
            Launching::NoAccountToSpare => Err(ApiError::new(ErrorCode::Conflict, NO_ACCOUNT_TO_SPARE)),
        }
    }

    /// The login a launch of a chat of `agent`'s kind that's `on` a login runs under, as [`Logins::to_hand`] says, but
    /// with Automatic finding no account to spare said apart, since the chat says so.
    pub(crate) fn to_launch(
        &self,
        agent: AgentKind,
        on: &SessionLogin,
        heard: &Heard,
        now_ms: i64,
    ) -> Result<Launching, ApiError> {
        let files = self.files.lock();
        let target = match self.target(agent, on, heard, now_ms)? {
            Aim::At(target) => target,
            Aim::Ambient => return Ok(Launching::Ambient),
            Aim::NoAccountToSpare => return Ok(Launching::NoAccountToSpare),
        };
        let handed = self.available(&files, agent, target.account(), now_ms, |called| target.needs_a_login(called))?;
        let placed = if matches!(target, Target::Automatic(_)) { Placed::Automatically } else { Placed::ByDefault };
        Ok(Launching::Handed(Handed { placed, ..handed }))
    }

    /// The account whose login a chat of `agent`'s kind that's `on` a login runs under at `now_ms`: the one it's on, or
    /// else the kind's default, or the one Automatic chooses, while the kind's chats are put on accounts automatically.
    fn target(&self, agent: AgentKind, on: &SessionLogin, heard: &Heard, now_ms: i64) -> Result<Aim, ApiError> {
        match on {
            SessionLogin::Hub(account) => return Ok(Aim::At(Target::Session(account.clone()))),
            SessionLogin::Ambient => return Ok(Aim::Ambient),
            SessionLogin::NoneYet => {}
        }
        let settings = self.store.hub_settings()?;
        let default = settings.account_defaults.unwrap_or_default().into_iter().find(|choice| choice.agent == agent);
        Ok(match default {
            Some(choice) if choice.automatic => self
                .automatic(agent, heard, now_ms)?
                .map_or(Aim::NoAccountToSpare, |account| Aim::At(Target::Automatic(account))),
            Some(choice) => Aim::At(Target::Default(choice.account)),
            None => Aim::Ambient,
        })
    }

    /// The account Automatic puts a chat of `agent`'s kind on at `now_ms`, of the logins added for the kind, by what's
    /// been `heard` of their limits, as [`automatic::choose`] says. Refused with `Conflict` when none has a login that
    /// works, which renewing or reconnecting one is what helps, and `None` when none has usage to spare.
    fn automatic(&self, agent: AgentKind, heard: &Heard, now_ms: i64) -> Result<Option<AccountId>, ApiError> {
        let mut accounts = Vec::new();
        for held in self.store.held_logins(agent)? {
            accounts.extend(self.store.account(&held.account)?.map(|account| automatic::with_heard(account, heard)));
        }
        if !accounts.iter().any(|account| account.works_for(agent, now_ms)) {
            return Err(ApiError::new(ErrorCode::Conflict, NO_LOGIN_WORKS));
        }
        let at_work = self.store.at_work_on_accounts()?;
        let candidates: Vec<Candidate<'_>> = accounts
            .iter()
            .map(|account| Candidate { account, working: at_work.get(&account.id).copied().unwrap_or_default() })
            .collect();
        Ok(automatic::choose(agent, &candidates, now_ms).cloned())
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
            .secret()
            .ok_or_else(unavailable)?;
        Ok(Handed { account: account.clone(), login: AgentLogin { token }, placed: Placed::ByDefault })
    }
}
