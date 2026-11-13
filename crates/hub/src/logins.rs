//! The logins the hub holds for agents' accounts. Each token is a file of its own, which `crate::login_files` keeps,
//! in the order it says, and everything else about the login is on its account's row in the database. The token stays
//! out of the database since the copies of it from before each migration would keep a token once it was disconnected.

use std::path::Path;
use std::sync::Arc;

use protocol::client::accounts::AccountConnect;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{Account, AccountId, AgentKind, AgentLogin, LoginDescription, LoginToken};

use crate::error::internal;
use crate::login_files::{LoginFiles, LoginSecret};
use crate::store::{Connecting, Held, IdFrom, Store, StoreError};

#[cfg(test)]
mod tests;

/// Longer than any name a person gives an account, which every client is sent on its record.
const MAX_NAME_BYTES: usize = 200;

/// Longer than any token an agent's CLI makes, which `claude setup-token`'s is about a hundred characters.
const MAX_TOKEN_BYTES: usize = 4096;

/// Connects, disconnects, tidies and hands out a login one at a time, under its files' lock, so none finds another half
/// done. Synchronous, and only called off the async threads.
pub(crate) struct Logins {
    store: Arc<Store>,
    files: LoginFiles<LoginToken>,
}

/// A token is kept as its text, and a file that isn't text holds none, since no token the hub wrote is.
impl LoginSecret for LoginToken {
    const DIR: &'static str = protocol::paths::LOGINS_DIR;

    fn to_bytes(&self) -> Vec<u8> {
        self.as_str().as_bytes().to_vec()
    }

    fn from_bytes(bytes: Vec<u8>) -> Option<Self> {
        String::from_utf8(bytes).ok().map(LoginToken::new)
    }
}

/// A login the hub gives an agent it launches, or a one-off call, with the account it holds it for.
#[derive(Debug)]
pub(crate) struct Handed {
    pub(crate) account: AccountId,
    pub(crate) login: AgentLogin,
}

/// Whether `held` still works at `now_ms`: it ends later, or nothing says when it ends.
fn not_ended(held: &Held, now_ms: i64) -> bool {
    held.login.ends_at_ms.is_none_or(|ends_at_ms| ends_at_ms > now_ms)
}

/// A login to connect, as a client asked: checked, trimmed and given the ID a new account would get.
pub(crate) struct Connect {
    agent: AgentKind,
    name: String,
    token: LoginToken,
    /// The account whose login it replaces, or `None` for a new one, or the one already holding the token.
    account: Option<AccountId>,
    new_id: AccountId,
    connected_at_ms: i64,
    ends_at_ms: Option<i64>,
}

impl Connect {
    /// Trims the token and the name, since what's pasted often ends in a line break, and refuses what can't be
    /// connected: an empty or very long name, and a token that's empty, very long, or has a space or a control
    /// character inside, which no CLI's has and an environment variable can't always carry. `login` is what the kind's
    /// description says of its login, which says when it ends.
    pub(crate) fn new(asked: AccountConnect, login: &LoginDescription, connected_at_ms: i64) -> Result<Self, ApiError> {
        let AccountConnect { agent, name, token, account } = asked;
        let refused = |message: &str| Err(ApiError::new(ErrorCode::InvalidRequest, message));
        let (name, token) = (name.trim(), token.as_str().trim());
        if name.is_empty() || name.len() > MAX_NAME_BYTES {
            return refused("the account needs a name, of at most 200 bytes");
        }
        if token.is_empty() || token.len() > MAX_TOKEN_BYTES {
            return refused("the token is empty, or longer than any a CLI makes");
        }
        if token.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return refused("the token has a space or a line break inside, which no CLI's has");
        }
        Ok(Self {
            agent,
            name: name.to_string(),
            token: LoginToken::new(token),
            account,
            new_id: AccountId::from(format!("{}-login-{}", agent.as_str(), uuid::Uuid::new_v4())),
            connected_at_ms,
            ends_at_ms: login.lasts_ms.map(|lasts_ms| connected_at_ms.saturating_add(lasts_ms)),
        })
    }
}

/// The account's record after a connect or a disconnect, and whether that changed it, so the hub announces only what
/// did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct AccountChange {
    pub(crate) account: Account,
    pub(crate) changed: bool,
}

impl Logins {
    /// The logins of a hub whose data directory is `data_dir`.
    pub(crate) fn new(store: Arc<Store>, data_dir: &Path) -> Self {
        Self { store, files: LoginFiles::new(data_dir) }
    }

    /// Has the hub hold `connect`'s login, as `AccountConnect` says. A token it already holds changes only the name of
    /// the account that holds it, when that's the account named, and without one finds that account under its own name
    /// and is refused under another. Otherwise the token is kept, and then the row written, as
    /// [`Locked::connect`](crate::login_files::Locked::connect) says.
    pub(crate) fn connect(&self, connect: &Connect) -> Result<AccountChange, ApiError> {
        let files = self.files.lock();
        let held = self.store.held_logins(connect.agent)?.into_iter().map(|held| (held.account, held.file));
        let id = match (&connect.account, files.holding(held, &connect.token)) {
            (None, Some(holding)) => return self.connected_already(&holding, &connect.name),
            (None, None) => &connect.new_id,
            (Some(id), holding) => {
                self.renewable(id, connect.agent)?;
                match holding {
                    Some(holding) if holding == *id => return self.rename(id, &connect.name),
                    Some(other) => {
                        let message = format!("the account {other} holds that token already");
                        return Err(ApiError::new(ErrorCode::Conflict, message));
                    }
                    None => id,
                }
            }
        };
        let account = files.connect(&connect.token, |file| {
            let connecting = Connecting {
                account: id,
                agent: connect.agent,
                name: &connect.name,
                file,
                connected_at_ms: connect.connected_at_ms,
                ends_at_ms: connect.ends_at_ms,
            };
            Ok(self.store.connect_login(&connecting)?)
        })?;
        Ok(AccountChange { account, changed: true })
    }

    /// Has the hub forget the login it holds for `id`, as `AccountDisconnect` says. The file goes first, and then the
    /// row, as [`Locked::disconnect`](crate::login_files::Locked::disconnect) says.
    pub(crate) fn disconnect(&self, id: &AccountId) -> Result<AccountChange, ApiError> {
        let files = self.files.lock();
        if let Some(file) = self.store.login_row(id)?.file
            && let Some(account) = files.disconnect(&file, || Ok(self.store.clear_login(id)?))?
        {
            return Ok(AccountChange { account, changed: true });
        }
        Ok(AccountChange { account: self.record(id)?, changed: false })
    }

    /// Whether the hub holds a login for agents of `agent`'s kind that hasn't ended at `now_ms`, as the accounts' rows
    /// say, without reading any token. So whether a launch would be handed one is known before its machine is asked
    /// whether it has its own, and a token that's gone fails only a launch that would be handed it.
    pub(crate) fn holds(&self, agent: AgentKind, now_ms: i64) -> Result<bool, ApiError> {
        Ok(self.store.held_logins(agent)?.iter().any(|held| not_ended(held, now_ms)))
    }

    /// The login to hand an agent of `agent`'s kind launched at `now_ms` on a machine with none of its own: of those
    /// the hub holds for that kind, the one connected most recently that hasn't ended, or `None` when it holds none
    /// such. One whose token is gone fails, rather than the agent running under an older login or none. A disconnect
    /// whose row couldn't be cleared leaves one, until it's tried again or the hub next starts and tidies, as does a
    /// file something else removed.
    pub(crate) fn to_hand(&self, agent: AgentKind, now_ms: i64) -> Result<Option<Handed>, ApiError> {
        let files = self.files.lock();
        let held = self.store.held_logins(agent)?;
        let Some(Held { account, name, file, .. }) = held.into_iter().find(|held| not_ended(held, now_ms)) else {
            return Ok(None);
        };
        let called = name.unwrap_or_else(|| account.to_string());
        match files.read(&file) {
            Ok(Some(token)) => Ok(Some(Handed { account, login: AgentLogin { token } })),
            Ok(None) => Err(internal(format!(
                "the token of the hub's login for {called} is gone, so connect it again, or disconnect it"
            ))),
            Err(e) => Err(internal(format!("couldn't read the token of the hub's login for {called}: {e}"))),
        }
    }

    /// Makes the logins' files and the accounts' rows agree, as the hub starts, as
    /// [`Locked::tidy`](crate::login_files::Locked::tidy) says.
    pub(crate) fn tidy(&self) -> Result<(), ApiError> {
        let files = self.files.lock();
        files.tidy(self.store.login_files()?, |id| {
            self.store.clear_login(id)?;
            Ok(())
        })
    }

    /// Refuses to replace the login of an account that has no record, or whose ID the hub didn't choose for a login of
    /// this kind, since nothing says the token is that account's. One whose login was disconnected can have one again.
    fn renewable(&self, id: &AccountId, agent: AgentKind) -> Result<(), ApiError> {
        if self.store.login_row(id)?.id_from == IdFrom::Hub(agent) {
            return Ok(());
        }
        let message =
            "the hub didn't choose that account's ID for a login of that kind, so nothing says the token is its";
        Err(ApiError::new(ErrorCode::InvalidRequest, message))
    }

    /// What connecting a token the hub already holds for `holding` does without naming an account. Under the same name
    /// it's the same again, as when a client didn't hear the answer, so it finds the account and changes nothing.
    /// Under another, it's refused: the person means to add an account, and renaming the one that holds the token
    /// would look as if they had.
    fn connected_already(&self, holding: &AccountId, name: &str) -> Result<AccountChange, ApiError> {
        let account = self.record(holding)?;
        if account.name.as_deref() == Some(name) {
            return Ok(AccountChange { account, changed: false });
        }
        let called = account.name.as_deref().unwrap_or(holding.as_str());
        Err(ApiError::new(ErrorCode::Conflict, format!("that token is connected already, as {called}")))
    }

    fn rename(&self, id: &AccountId, name: &str) -> Result<AccountChange, ApiError> {
        Ok(match self.store.rename_account(id, name)? {
            Some(account) => AccountChange { account, changed: true },
            None => AccountChange { account: self.record(id)?, changed: false },
        })
    }

    fn record(&self, id: &AccountId) -> Result<Account, ApiError> {
        let found = self.store.account(id)?;
        Ok(found.ok_or_else(|| StoreError::NotFound { kind: "account", id: id.to_string() })?)
    }
}
