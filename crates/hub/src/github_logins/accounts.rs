use std::path::Path;
use std::sync::Arc;

use protocol::client::github_accounts::GithubAccountConnect;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GithubAccount, GithubAccountId, GithubLoginKind, GithubToken, GithubUser, HubSettings};

use crate::error::internal;
use crate::github::TokenOwner;
use crate::login_files::{Locked, LoginFiles, LoginSecret};
use crate::pasted_token::pasted_token;
use crate::store::{GithubConnected, GithubConnecting, Store, StoreError};

/// Connects, disconnects and tidies the logins of GitHub accounts, and chooses the default, one at a time, under their
/// files' lock, so none finds another half done. Synchronous, and only called off the async threads. Nothing here
/// calls GitHub, which is asked before the lock is taken.
pub(crate) struct GithubLogins {
    pub(super) store: Arc<Store>,
    pub(super) files: LoginFiles<GithubToken>,
}

/// A token is kept as its text, and a file that isn't text holds none, since no token the hub wrote is.
impl LoginSecret for GithubToken {
    const DIR: &'static str = protocol::paths::GITHUB_LOGINS_DIR;

    fn to_bytes(&self) -> Vec<u8> {
        self.as_str().as_bytes().to_vec()
    }

    fn from_bytes(bytes: Vec<u8>) -> Option<Self> {
        String::from_utf8(bytes).ok().map(GithubToken::new)
    }
}

/// A token to connect, as a client asked: trimmed, and given the ID a new account would get.
pub(crate) struct Connect {
    pub(super) token: GithubToken,
    /// The account whose login it renews, or `None` for a new one, or the one already holding the token.
    pub(super) id: Option<GithubAccountId>,
    pub(super) new_id: GithubAccountId,
    pub(super) connected_at_ms: i64,
}

impl Connect {
    /// Refuses a token that can't be one, as [`pasted_token`] says.
    pub(crate) fn new(asked: GithubAccountConnect, connected_at_ms: i64) -> Result<Self, ApiError> {
        let GithubAccountConnect { token, id } = asked;
        let token = GithubToken::new(pasted_token(token.as_str())?);
        let new_id = GithubAccountId::from(uuid::Uuid::new_v4().to_string());
        Ok(Self { token, id, new_id, connected_at_ms })
    }

    pub(crate) fn token(&self) -> &GithubToken {
        &self.token
    }
}

/// The account's record after a connect or a disconnect, whether that changed it, and the settings when it changed the
/// default, so the hub announces only what did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GithubAccountChange {
    pub(crate) account: GithubAccount,
    pub(crate) changed: bool,
    pub(crate) settings: Option<HubSettings>,
}

impl GithubAccountChange {
    fn unchanged(account: GithubAccount) -> Self {
        Self { account, changed: false, settings: None }
    }
}

/// What connecting a token comes to, as the files and the rows say.
enum Plan {
    /// An account holds it already, as when a client sends the same again, so nothing changes.
    Held(GithubAccount),
    /// Kept as the login of this account, made if it's new.
    Keep(GithubAccountId),
}

impl GithubLogins {
    /// The logins of a hub whose data directory is `data_dir`.
    pub(crate) fn new(store: Arc<Store>, data_dir: &Path) -> Self {
        Self { store, files: LoginFiles::new(data_dir) }
    }

    /// What connecting `connect` comes to without asking GitHub: the account that holds its token already, which
    /// connecting it again changes nothing of, or `None`, when GitHub has to be asked whose it is. Refused as
    /// [`GithubLogins::connect`] would refuse it without knowing whose it is.
    pub(crate) fn held(&self, connect: &Connect) -> Result<Option<GithubAccountChange>, ApiError> {
        let files = self.files.lock();
        Ok(match self.plan(&files, connect, None)? {
            Plan::Held(account) => Some(GithubAccountChange::unchanged(account)),
            Plan::Keep(_) => None,
        })
    }

    /// Has the hub hold `connect`'s token, which GitHub said is `owner`'s, as `GithubAccountConnect` says. What GitHub
    /// said is checked again under the lock, since another connect can have come between. The token is kept, and then
    /// the row written, as [`Locked::connect`] says.
    pub(crate) fn connect(&self, connect: &Connect, owner: &TokenOwner) -> Result<GithubAccountChange, ApiError> {
        let files = self.files.lock();
        let id = match self.plan(&files, connect, Some(&owner.user))? {
            Plan::Held(account) => return Ok(GithubAccountChange::unchanged(account)),
            Plan::Keep(id) => id,
        };
        let (account, settings) = files.connect(&connect.token, |file| {
            let connecting = GithubConnecting {
                account: &id,
                user: &owner.user,
                how: GithubLoginKind::Token,
                file,
                connected_at_ms: connect.connected_at_ms,
                ends_at_ms: owner.ends_at_ms,
            };
            let GithubConnected { account, replaced, settings } = self.store.connect_github_login(&connecting)?;
            Ok(((account, settings), replaced))
        })?;
        Ok(GithubAccountChange { account, changed: true, settings })
    }

    /// Has the hub forget the login it holds for `id`, keeping the record, as `GithubAccountDisconnect` says. The file
    /// goes first, and then the row, as [`Locked::disconnect`] says.
    pub(crate) fn disconnect(&self, id: &GithubAccountId) -> Result<GithubAccountChange, ApiError> {
        let files = self.files.lock();
        if let Some(file) = self.store.github_login_file(id)?
            && let Some(account) = files.disconnect(&file, || Ok(self.store.clear_github_login(id)?))?
        {
            return Ok(GithubAccountChange { account, changed: true, settings: None });
        }
        Ok(GithubAccountChange::unchanged(self.record(id)?))
    }

    /// Makes `account` the default, or each machine's own login with `None`, as `GithubAccountSetDefault` says, under
    /// the same lock that connects and disconnects, so the login it checks is still there as it's chosen.
    pub(crate) fn set_default(&self, account: Option<&GithubAccountId>, now_ms: i64) -> Result<HubSettings, ApiError> {
        let files = self.files.lock();
        if let Some(id) = account {
            self.usable(&files, id, now_ms)?;
        }
        Ok(self.store.set_github_default(account)?)
    }

    /// Makes the logins' files and the GitHub accounts' rows agree, as the hub starts, as [`Locked::tidy`] says.
    pub(crate) fn tidy(&self) -> Result<(), ApiError> {
        let files = self.files.lock();
        files.tidy(self.store.github_login_files()?, |id| {
            self.store.clear_github_login(id)?;
            Ok(())
        })
    }

    /// What connecting `connect` comes to, as the files and the rows say now, with `user` once GitHub has said whose
    /// the token is. A token another account holds is refused under `connect.id`, and so is, once GitHub has said, a
    /// token of another user than the account `connect.id` is.
    fn plan(
        &self,
        files: &Locked<'_, GithubToken>,
        connect: &Connect,
        user: Option<&GithubUser>,
    ) -> Result<Plan, ApiError> {
        if let Some(holding) = files.holding(self.store.held_github_logins()?, &connect.token) {
            return match &connect.id {
                Some(id) if *id != holding => Err(ApiError::new(
                    ErrorCode::Conflict,
                    format!("the GitHub account {holding} holds that token already"),
                )),
                _ => Ok(Plan::Held(self.record(&holding)?)),
            };
        }
        let Some(id) = &connect.id else { return Ok(Plan::Keep(connect.new_id.clone())) };
        let account = self.record(id)?;
        match user {
            Some(user) if user.id != account.user.id => {
                let message = format!("that token is {}'s on GitHub, not {}'s", user.name, account.user.name);
                Err(ApiError::new(ErrorCode::Conflict, message))
            }
            _ => Ok(Plan::Keep(id.clone())),
        }
    }

    /// Refuses with `Conflict` an account whose login can't be used now: disconnected, ended, or with no file that
    /// holds a token.
    fn usable(&self, files: &Locked<'_, GithubToken>, id: &GithubAccountId, now_ms: i64) -> Result<(), ApiError> {
        let account = self.record(id)?;
        let unusable = || {
            let message = format!("{} needs a login; renew or reconnect it first", account.user.name);
            ApiError::new(ErrorCode::Conflict, message)
        };
        if !account.held_login.is_some_and(|held| held.works_at(now_ms)) {
            return Err(unusable());
        }
        let file = self.store.github_login_file(id)?.ok_or_else(unusable)?;
        let token =
            files.read(&file).map_err(|e| internal(format!("couldn't read {}'s login: {e}", account.user.name)))?;
        token.map(drop).ok_or_else(unusable)
    }

    fn record(&self, id: &GithubAccountId) -> Result<GithubAccount, ApiError> {
        let found = self.store.github_account(id)?;
        Ok(found.ok_or_else(|| StoreError::NotFound { kind: "GitHub account", id: id.to_string() })?)
    }
}
