use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::github_accounts::GithubAccountConnect;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    GithubAccount, GithubAccountId, GithubAppId, GithubAppSlug, GithubInstallation, GithubLoginKind, GithubToken,
    GithubUser, GithubUserId, GithubUsername, HubSettings, UnusableLogin,
};
use tokio::sync::watch;

use super::blocks::Unusable;
use super::gh_login::GhKept;
use super::nobody::NobodyAsks;
use super::{DefaultCredential, GithubLogin};
use crate::error::internal;
use crate::hub::{GithubAppSaid as AppSaid, GithubUsername as Username, InstalledOn, Observations};
use crate::login_files::{Locked, LoginFiles};
use crate::pasted_token::pasted_token;
use crate::store::{GithubConnected, GithubConnecting, GithubRemoved, Store, StoreError};
use github::TokenOwner;

/// Connects, disconnects and tidies the logins of GitHub accounts, chooses the default, and approves and ends sign-ins
/// with GitHub, one at a time, under their files' lock, so none finds another half done, and works out after each
/// change to a login what workers give their processes for github.com, which it records on the hub's settings.
/// Synchronous, and only called off the async threads. Nothing here calls GitHub, which is asked before the lock is
/// taken, a worker, whose link follows [`GithubLogins::git_credential`] on its own, or a client, which the hub tells of
/// the settings as it follows [`GithubLogins::recorded_settings`].
pub(crate) struct GithubLogins {
    pub(super) store: Arc<Store>,
    /// The hub's observations, which hold what GitHub calls each account's user and what its profile says, which the
    /// logins read and the hub keeps, as `crate::hub::observations::GithubUsername` says.
    pub(super) observations: Arc<Observations>,
    pub(super) files: LoginFiles<GithubLogin>,
    /// What workers are to give their processes for github.com, with who GitHub knows the default as, as of the last
    /// change, which is made under the lock.
    pub(super) credential: watch::Sender<DefaultCredential>,
    /// The hub's settings as of the last time what workers are sent was recorded anew on them, under the lock.
    pub(super) recorded: watch::Sender<Option<HubSettings>>,
    /// What the hub last read of the login `gh` has on its machine, in memory alone, as `GithubLogins::gh_read` says.
    pub(super) gh: Mutex<Option<GhKept>>,
    /// When GitHub last refused an account's token as the hub asked who it is, by account, in memory alone, so it's
    /// asked with no login from then until it's connected again, as `GithubLogins::profile_read` says.
    pub(super) refused: Mutex<HashMap<GithubAccountId, i64>>,
    /// When the hub has asked GitHub who an account is with no login, as [`NobodyAsks`] says.
    pub(super) nobody: Mutex<NobodyAsks>,
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

    /// When it was connected, by the hub's clock, which is when GitHub was asked whose it is.
    pub(crate) fn connected_at_ms(&self) -> i64 {
        self.connected_at_ms
    }
}

/// The account's record after a connect or a disconnect, whether that changed it, and the settings when it changed the
/// default, so the hub announces only what did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GithubAccountChange {
    pub(crate) account: GithubAccount,
    pub(crate) changed: bool,
    pub(crate) settings: Option<HubSettings>,
}

impl GithubAccountChange {
    pub(super) fn unchanged(account: GithubAccount) -> Self {
        Self { account, changed: false, settings: None }
    }

    /// The same, with the settings it changed as they are once what workers are sent for the default they name is
    /// recorded on them too, rather than as the change read them just before, so the copy the hub announces never
    /// names a default with what's sent for the one before it.
    pub(crate) fn settled(self, store: &Store) -> Result<Self, ApiError> {
        let settings = self.settings.map(|_| store.hub_settings()).transpose()?;
        Ok(Self { settings, ..self })
    }
}

/// What disconnecting a GitHub account came to: an app's installation's record, kept with no login, or any other
/// account's removal.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Disconnected {
    Kept(GithubAccountChange),
    Removed(GithubRemoved),
}

/// What a file in the GitHub logins' directory is kept for, as tidying names it: an account's login, or an app's key.
enum KeptFor {
    Account(GithubAccountId),
    App(GithubAppId),
}

impl fmt::Display for KeptFor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeptFor::Account(id) => id.fmt(f),
            KeptFor::App(id) => write!(f, "the app {id}"),
        }
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
    /// The logins of a hub whose data directory is `data_dir`, with what workers are to be sent at `now_ms`, recorded
    /// on the hub's settings, before any client can read them.
    pub(crate) fn new(store: Arc<Store>, observations: Arc<Observations>, data_dir: &Path, now_ms: i64) -> Self {
        // Replaced before this returns, so before anything can follow it. A block, so even a mistake there fails closed.
        let reason = "Roundhouse hasn't read its GitHub accounts".to_string();
        let unread = DefaultCredential::Blocked { reason, account: None, why: UnusableLogin::Unreadable };
        let logins = Self {
            store,
            observations,
            files: LoginFiles::new(data_dir),
            credential: watch::Sender::new(unread),
            recorded: watch::Sender::new(None),
            gh: Mutex::new(None),
            refused: Mutex::new(HashMap::new()),
            nobody: Mutex::new(NobodyAsks::default()),
        };
        logins.publish(&logins.files.lock(), now_ms);
        logins
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
        self.changing(connect.connected_at_ms, |files| {
            let id = match self.plan(files, connect, Some(&owner.user))? {
                Plan::Held(account) => return Ok(GithubAccountChange::unchanged(account)),
                Plan::Keep(id) => id,
            };
            let (account, settings) = files.connect(&GithubLogin::Token(connect.token.clone()), |file| {
                let connecting = GithubConnecting {
                    account: &id,
                    user: &owner.user,
                    how: GithubLoginKind::Token,
                    file,
                    connected_at_ms: connect.connected_at_ms,
                    ends_at_ms: owner.ends_at_ms,
                    installation: None,
                };
                let GithubConnected { account, replaced, settings } = self.store.connect_github_login(&connecting)?;
                Ok(((account, settings), replaced))
            })?;
            Ok(GithubAccountChange { account, changed: true, settings })
        })?
        .settled(&self.store)
    }

    /// Has the hub forget the login it holds for `id`, as `GithubAccountDisconnect` says, at `now_ms`, and the account
    /// with it, unless it's an app's installation, whose record stays until its app is removed, so the hub doesn't
    /// connect it again by itself. The file goes first, and then the row, as [`Locked::disconnect`] says. An account
    /// that's gone is refused as not found, as one removed before is.
    pub(crate) fn disconnect(&self, id: &GithubAccountId, now_ms: i64) -> Result<Disconnected, ApiError> {
        let disconnected = self.changing(now_ms, |files| {
            let account = self.record(id)?;
            let file = self.store.github_login_file(id)?;
            if account.installation.is_some() {
                if let Some(file) = file
                    && let Some(account) = files.disconnect(&file, || Ok(self.store.clear_github_login(id)?))?
                {
                    return Ok(Disconnected::Kept(GithubAccountChange { account, changed: true, settings: None }));
                }
                return Ok(Disconnected::Kept(GithubAccountChange::unchanged(account)));
            }
            let remove = || {
                let gone = || StoreError::NotFound { kind: "GitHub account", id: id.to_string() };
                Ok(self.store.remove_github_account(id)?.ok_or_else(gone)?)
            };
            let removed = match file {
                Some(file) => files.disconnect(&file, remove)?,
                None => remove()?,
            };
            if !self.holds_gh()? {
                *self.gh.lock() = None;
            }
            self.refused.lock().remove(id);
            Ok(Disconnected::Removed(removed))
        })?;
        match disconnected {
            Disconnected::Removed(GithubRemoved { account, rev, settings }) => {
                let settings = settings.map(|_| self.store.hub_settings()).transpose()?;
                Ok(Disconnected::Removed(GithubRemoved { account, rev, settings }))
            }
            kept @ Disconnected::Kept(_) => Ok(kept),
        }
    }

    /// Makes `account` the default, or each machine's own login with `None`, as `GithubAccountSetDefault` says, under
    /// the same lock that connects and disconnects, so the login it checks is still there as it's chosen.
    /// Answers with the settings as they are once what workers are sent for it is recorded on them too.
    pub(crate) fn set_default(&self, account: Option<&GithubAccountId>, now_ms: i64) -> Result<HubSettings, ApiError> {
        self.changing(now_ms, |files| {
            if let Some(id) = account {
                self.usable(files, id, now_ms)?;
            }
            self.store.set_github_default(account)?;
            Ok(())
        })?;
        Ok(self.store.hub_settings()?)
    }

    /// Makes the logins' files and the GitHub accounts' rows agree, as the hub starts, at `now_ms`, as
    /// [`Locked::tidy`] says.
    pub(crate) fn tidy(&self, now_ms: i64) -> Result<(), ApiError> {
        self.changing(now_ms, |files| {
            let accounts = self.store.github_login_files()?.into_iter().map(|(id, file)| (KeptFor::Account(id), file));
            let apps = self.store.github_app_keys()?.into_iter().map(|(id, file)| (KeptFor::App(id), file));
            files.tidy(accounts.chain(apps).collect(), |kept_for| match kept_for {
                // As disconnecting it would have, had the hub not stopped between letting go of its file and its row.
                KeptFor::Account(id) => {
                    let installation =
                        self.store.github_account(id)?.is_some_and(|account| account.installation.is_some());
                    if installation {
                        self.store.clear_github_login(id)?;
                    } else {
                        self.store.remove_github_account(id)?;
                    }
                    Ok(())
                }
                // As removing it would have, had the hub not stopped between letting go of its key and its record.
                KeptFor::App(id) => {
                    self.store.remove_github_app(*id)?;
                    Ok(())
                }
            })
        })
    }

    /// What connecting `connect` comes to, as the files and the rows say now, with `user` once GitHub has said whose
    /// the token is. A token another account holds is refused under `connect.id`, and so is, once GitHub has said, a
    /// token of another user than the account `connect.id` is.
    fn plan(
        &self,
        files: &Locked<'_, GithubLogin>,
        connect: &Connect,
        user: Option<&GithubUser>,
    ) -> Result<Plan, ApiError> {
        let token = GithubLogin::Token(connect.token.clone());
        if let Some(holding) = files.holding(self.store.held_github_logins()?, &token) {
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
                let message = format!("that token is {}'s on GitHub, not {}'s", user.name, self.named(&account));
                Err(ApiError::new(ErrorCode::Conflict, message))
            }
            _ => Ok(Plan::Keep(id.clone())),
        }
    }

    /// Refuses with `Conflict` an account whose login can't be used now: disconnected, ended, needing signing in again,
    /// being renewed, in a form only a newer build reads, or with no file that holds a token.
    fn usable(&self, files: &Locked<'_, GithubLogin>, id: &GithubAccountId, now_ms: i64) -> Result<(), ApiError> {
        let account = self.record(id)?;
        let name = self.named(&account);
        match self.token_of(files, &account, now_ms)? {
            Ok(_) => Ok(()),
            Err(Unusable::Unreadable(Some(e))) => Err(internal(format!("couldn't read {name}'s login: {e}"))),
            Err(Unusable::Disconnected | Unusable::Ended | Unusable::Unreadable(None)) => {
                let message = format!("{name} needs a login; renew or reconnect it first");
                Err(ApiError::new(ErrorCode::Conflict, message))
            }
            Err(Unusable::SignInAgain) => {
                let message = format!("{name} needs signing in with GitHub again first");
                Err(ApiError::new(ErrorCode::Conflict, message))
            }
            Err(Unusable::Renewing) => {
                let renewing = if account.installation.is_some() { "token" } else { "sign-in with GitHub" };
                let message = format!("{name}'s {renewing} is being renewed; try again in a minute");
                Err(ApiError::new(ErrorCode::Conflict, message))
            }
            Err(Unusable::Newer) => {
                let message = format!("{name}'s login is one only a newer Roundhouse can read");
                Err(ApiError::new(ErrorCode::Conflict, message))
            }
            Err(Unusable::GhLoggedOut(why)) => {
                let message = format!("gh on the hub's machine has no login that works for {name} ({why})");
                Err(ApiError::new(ErrorCode::Conflict, message))
            }
            Err(Unusable::GhOtherUser(now)) => {
                let message = format!("gh on the hub's machine is logged in as {now} now, not {name}");
                Err(ApiError::new(ErrorCode::Conflict, message))
            }
            Err(Unusable::GhUnread(_)) => {
                let message = format!("the hub couldn't read gh's login for {name} lately; try again in a minute");
                Err(ApiError::new(ErrorCode::Conflict, message))
            }
        }
    }

    pub(super) fn record(&self, id: &GithubAccountId) -> Result<GithubAccount, ApiError> {
        let found = self.store.github_account(id)?;
        Ok(found.ok_or_else(|| StoreError::NotFound { kind: "GitHub account", id: id.to_string() })?)
    }

    /// What GitHub calls `account`'s user, as the hub last heard it, or its number, as `#<number>`, while that isn't
    /// known, for what the logins say of it to a person.
    pub(super) fn named(&self, account: &GithubAccount) -> GithubUsername {
        self.observations.value_of::<Username>(&account.id).unwrap_or_else(|| by_number(account.user.id))
    }

    /// What GitHub calls the user or organization `installation`, `account`'s, is on, as the hub last heard it, or its
    /// number while that isn't known, as [`GithubLogins::named`] says.
    pub(super) fn installed_on(&self, account: &GithubAccount, installation: &GithubInstallation) -> GithubUsername {
        self.observations.value_of::<InstalledOn>(&account.id).unwrap_or_else(|| by_number(installation.on.id))
    }

    /// What GitHub calls the app of your own `app` in addresses, as the hub last heard it, or its number while that
    /// isn't known, as [`GithubLogins::named`] says.
    pub(super) fn slug_of(&self, app: GithubAppId) -> GithubAppSlug {
        let said = self.observations.value_of::<AppSaid>(&app);
        said.map_or_else(|| GithubAppSlug::from(format!("#{app}").as_str()), |said| said.slug)
    }
}

/// A user or organization named by its number, as `#<number>`, while what GitHub calls it isn't known.
fn by_number(id: GithubUserId) -> GithubUsername {
    GithubUsername::from(format!("#{id}").as_str())
}
