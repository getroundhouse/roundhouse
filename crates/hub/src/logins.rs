//! The logins the hub holds for agents' accounts. Each token is a file of its own, which `crate::login_files` keeps,
//! and everything else about the login is on its account's row in the database. The token stays out of the database
//! since the copies of it from before each migration would keep a token once it was disconnected.
//!
//! The writes are in an order chosen so that whatever stops the hub, or the machine, leaves either a file no row names
//! or a row naming a file that's gone. As the hub starts, [`Logins::tidy`] mends both. So a login the hub has said it
//! holds is never lost, and a token it was told to forget never comes back.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::accounts::AccountConnect;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{Account, AccountId, AgentKind, AgentLogin, LoginDescription, LoginToken};
use tracing::{info, warn};

use crate::error::internal;
use crate::login_files::{LoginFile, LoginFiles};
use crate::store::{Connecting, Held, IdFrom, Store, StoreError};

#[cfg(test)]
mod tests;

/// Longer than any name a person gives an account, which every client is sent on its record.
const MAX_NAME_BYTES: usize = 200;

/// Longer than any token an agent's CLI makes, which `claude setup-token`'s is about a hundred characters.
const MAX_TOKEN_BYTES: usize = 4096;

/// Connects, disconnects, tidies and hands out a login one at a time, so none finds another half done. Synchronous, and
/// only called off the async threads.
pub(crate) struct Logins {
    store: Arc<Store>,
    files: LoginFiles,
    /// A `parking_lot` lock held through file syncs and a synced commit, which is right only because it's only taken
    /// off the async threads, as the store's own are, and guards nothing in memory.
    one_at_a_time: Mutex<()>,
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
    pub(crate) fn new(store: Arc<Store>, dir: PathBuf) -> Self {
        Self { store, files: LoginFiles::new(dir), one_at_a_time: Mutex::new(()) }
    }

    /// Has the hub hold `connect`'s login, as `AccountConnect` says. A token it already holds changes only the name of
    /// the account that holds it, when that's the account named, and without one finds that account under its own name
    /// and is refused under another. Otherwise the token's file is written, and then the row, so a hub that stops
    /// between the two leaves a file no row names.
    pub(crate) fn connect(&self, connect: &Connect) -> Result<AccountChange, ApiError> {
        let _one_at_a_time = self.one_at_a_time.lock();
        let holding = self.holding(connect.agent, &connect.token)?;
        let id = match (&connect.account, holding) {
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
        let file = LoginFile::new();
        self.files.write(&file, &connect.token).map_err(|e| {
            self.let_go(&file);
            internal(format!("couldn't keep the login's token: {e}"))
        })?;
        let connecting = Connecting {
            account: id,
            agent: connect.agent,
            name: &connect.name,
            file: &file,
            connected_at_ms: connect.connected_at_ms,
            ends_at_ms: connect.ends_at_ms,
        };
        let (account, replaced) = self.store.connect_login(&connecting).inspect_err(|_| self.let_go(&file))?;
        if let Some(replaced) = replaced {
            self.let_go(&replaced);
        }
        Ok(AccountChange { account, changed: true })
    }

    /// Has the hub forget the login it holds for `id`, as `AccountDisconnect` says. The file goes first, and then the
    /// row, so nothing that stops the hub or the machine between the two brings the token back.
    pub(crate) fn disconnect(&self, id: &AccountId) -> Result<AccountChange, ApiError> {
        let _one_at_a_time = self.one_at_a_time.lock();
        if let Some(file) = self.store.login_row(id)?.file {
            self.files.remove(&file).map_err(|e| internal(format!("couldn't forget the login's token: {e}")))?;
            if let Some(account) = self.store.clear_login(id)? {
                return Ok(AccountChange { account, changed: true });
            }
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
        let _one_at_a_time = self.one_at_a_time.lock();
        let held = self.store.held_logins(agent)?;
        let Some(Held { account, name, file, .. }) = held.into_iter().find(|held| not_ended(held, now_ms)) else {
            return Ok(None);
        };
        let called = name.unwrap_or_else(|| account.to_string());
        match self.files.read(&file) {
            Ok(Some(token)) => Ok(Some(Handed { account, login: AgentLogin { token } })),
            Ok(None) => Err(internal(format!(
                "the token of the hub's login for {called} is gone, so connect it again, or disconnect it"
            ))),
            Err(e) => Err(internal(format!("couldn't read the token of the hub's login for {called}: {e}"))),
        }
    }

    /// Makes the files and the rows agree, as the hub starts: a row whose file is gone, empty or not a token holds no
    /// login, and a file no row names is removed. A file that can't be read is left with its row, and said, so the
    /// rest are still tidied.
    pub(crate) fn tidy(&self) -> Result<(), ApiError> {
        let _one_at_a_time = self.one_at_a_time.lock();
        let mut kept = HashSet::new();
        for (id, file) in self.store.login_files()? {
            let held = match &file {
                Some(file) => self.files.read(file).map(|token| token.is_some()),
                None => Ok(false),
            };
            match (file, held) {
                (Some(file), Ok(true)) => {
                    kept.insert(file);
                }
                (Some(file), Err(e)) => {
                    warn!(account = %id, "couldn't read the token of the hub's login for an account: {e}");
                    kept.insert(file);
                }
                (None, _) | (Some(_), Ok(false)) => {
                    info!(account = %id, "the token of the hub's login for an account is gone, so it holds none");
                    self.store.clear_login(&id)?;
                }
            }
        }
        self.files.remove_all_but(&kept).map_err(|e| internal(format!("couldn't let go of the logins' files: {e}")))
    }

    /// The account whose login of `agent`'s kind has `token`, if the hub holds one. A file that can't be read is said
    /// and passed over, so one account's file doesn't stop another's login being connected.
    fn holding(&self, agent: AgentKind, token: &LoginToken) -> Result<Option<AccountId>, ApiError> {
        for Held { account, file, .. } in self.store.held_logins(agent)? {
            match self.files.read(&file) {
                Ok(read) if read.as_ref() == Some(token) => return Ok(Some(account)),
                Ok(_) => {}
                Err(e) => warn!(%account, "couldn't read the token of the hub's login for an account: {e}"),
            }
        }
        Ok(None)
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

    /// Removes a file no row names, or will. One that can't be is left for [`Logins::tidy`] as the hub next starts,
    /// since nothing reads a file no row names, so what was asked still counts.
    fn let_go(&self, file: &LoginFile) {
        if let Err(e) = self.files.remove(file) {
            warn!("couldn't let go of a login's file no account names, which goes as the hub next starts: {e}");
        }
    }
}
