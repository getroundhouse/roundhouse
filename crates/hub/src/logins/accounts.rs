use crate::login_files::{LoginFiles, LoginSecret, Stored};
use crate::pasted_token::pasted_token;
use crate::store::{Connected, Connecting, IdFrom, Placed, Store, StoreError};
use protocol::client::accounts::AccountConnect;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{Account, AccountId, AgentKind, AgentLogin, HubSettings, LoginDescription, LoginToken};
use std::path::Path;
use std::sync::Arc;

/// Longer than any name a person gives an account, which every client is sent on its record.
const MAX_NAME_BYTES: usize = 200;

/// Connects, disconnects, tidies and hands out a login one at a time, under its files' lock, so none finds another half
/// done. Synchronous, and only called off the async threads.
pub(crate) struct Logins {
    pub(super) store: Arc<Store>,
    pub(super) files: LoginFiles<LoginToken>,
}

/// A token is kept as its text, and a file that isn't text holds none, since no token the hub wrote is.
impl LoginSecret for LoginToken {
    const DIR: &'static str = protocol::paths::LOGINS_DIR;

    fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        Ok(self.as_str().as_bytes().to_vec())
    }

    fn from_bytes(bytes: Vec<u8>) -> Stored<Self> {
        String::from_utf8(bytes).map_or(Stored::Nothing, |token| Stored::Secret(LoginToken::new(token)))
    }
}

/// A login the hub gives an agent it launches, or a one-off call, with the account it holds it for.
#[derive(Debug)]
pub(crate) struct Handed {
    pub(crate) account: AccountId,
    pub(crate) login: AgentLogin,
    /// What put a chat on none yet on the account, which the launch records on the chat.
    pub(crate) placed: Placed,
}

/// A login to connect, as a client asked: checked, trimmed and given the ID a new account would get.
pub(crate) struct Connect {
    pub(super) agent: AgentKind,
    pub(super) name: String,
    pub(super) token: LoginToken,
    /// The account whose login it replaces, or `None` for a new one, or the one already holding the token.
    pub(super) account: Option<AccountId>,
    /// The hub's own ID for an account whose token doesn't say whose it is. It starts with the kind's name, as an
    /// adapter's ID does, and `-login-` follows, which no adapter writes, so it can't be the same as an adapter's.
    pub(super) new_id: AccountId,
    pub(super) connected_at_ms: i64,
    pub(super) ends_at_ms: Option<i64>,
}

impl Connect {
    /// Trims the token and the name, since what's pasted often ends in a line break, and refuses what can't be
    /// connected: an empty or very long name, and a token that can't be one, as [`pasted_token`] says. `login` is what
    /// the kind's description says of its login, which says when it ends.
    pub(crate) fn new(asked: AccountConnect, login: &LoginDescription, connected_at_ms: i64) -> Result<Self, ApiError> {
        let AccountConnect { agent, name, token, account } = asked;
        let name = name.trim();
        if name.is_empty() || name.len() > MAX_NAME_BYTES {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "the account needs a name, of at most 200 bytes"));
        }
        let token = pasted_token(token.as_str())?;
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
    pub(crate) defaults: Option<HubSettings>,
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
        let (account, defaults) = files.connect(&connect.token, |file| {
            let connecting = Connecting {
                account: id,
                agent: connect.agent,
                name: &connect.name,
                file,
                connected_at_ms: connect.connected_at_ms,
                ends_at_ms: connect.ends_at_ms,
            };
            let Connected { account, replaced, defaults } = self.store.connect_login(&connecting)?;
            Ok(((account, defaults), replaced))
        })?;
        Ok(AccountChange { account, changed: true, defaults })
    }

    /// Has the hub forget the login it holds for `id`, as `AccountDisconnect` says. The file goes first, and then the
    /// row, as [`Locked::disconnect`](crate::login_files::Locked::disconnect) says.
    pub(crate) fn disconnect(&self, id: &AccountId) -> Result<AccountChange, ApiError> {
        let files = self.files.lock();
        if let Some(file) = self.store.login_row(id)?.file
            && let Some(account) = files.disconnect(&file, || Ok(self.store.clear_login(id)?))?
        {
            return Ok(AccountChange { account, changed: true, defaults: None });
        }
        Ok(AccountChange { account: self.record(id)?, changed: false, defaults: None })
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
    pub(super) fn renewable(&self, id: &AccountId, agent: AgentKind) -> Result<(), ApiError> {
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
    pub(super) fn connected_already(&self, holding: &AccountId, name: &str) -> Result<AccountChange, ApiError> {
        let account = self.record(holding)?;
        if account.name.as_deref() == Some(name) {
            return Ok(AccountChange { account, changed: false, defaults: None });
        }
        let called = account.name.as_deref().unwrap_or(holding.as_str());
        Err(ApiError::new(ErrorCode::Conflict, format!("that token is connected already, as {called}")))
    }

    pub(super) fn rename(&self, id: &AccountId, name: &str) -> Result<AccountChange, ApiError> {
        Ok(match self.store.rename_account(id, name)? {
            Some(account) => AccountChange { account, changed: true, defaults: None },
            None => AccountChange { account: self.record(id)?, changed: false, defaults: None },
        })
    }

    pub(super) fn record(&self, id: &AccountId) -> Result<Account, ApiError> {
        let found = self.store.account(id)?;
        Ok(found.ok_or_else(|| StoreError::NotFound { kind: "account", id: id.to_string() })?)
    }
}
