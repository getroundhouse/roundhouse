//! The logins of an app's installations: which of those GitHub lists to connect, forgetting those it no longer lists,
//! connecting each with a token made from the app's key, and making each a new token before its last ends. The hub asks
//! GitHub between these, never under the logins' lock.

use std::collections::HashMap;
use std::time::Duration;

use protocol::client::ApiError;
use protocol::model::{
    GithubAccount, GithubAccountId, GithubApp, GithubAppId, GithubInstallation, GithubInstallationId, GithubLoginKind,
    GithubUser,
};
use tracing::{info, warn};

use super::{GithubAccountChange, GithubLogin, GithubLogins, InstallationToken};
use crate::login_files::{Locked, Stored};
use crate::store::{GithubConnected, GithubConnecting, later_ms};
use github::{GithubAppKey, Installation, Minted};

/// How much an installation's token has left when the hub makes the next. GitHub's last an hour, so the next is made
/// about every 40 minutes, and the check every minute tries twenty times before the token ends. Making one takes
/// nothing from the last, so making it early costs only a call. *Unconfirmed.*
pub(crate) const MINTED_WITH_LEFT: Duration = Duration::from_mins(20);

/// An installation's token to make again, as a check found it: its account, which installation that is, and its
/// app's key to ask with.
#[derive(Debug)]
pub(crate) struct MintDue {
    pub(crate) account: GithubAccountId,
    pub(crate) app: GithubAppId,
    pub(crate) installation: GithubInstallationId,
    pub(crate) key: GithubAppKey,
}

/// What looking at an app's installations found: which to make a token for and connect, and the record of each
/// account whose login it forgot, as one GitHub no longer lists.
#[derive(Debug, Default)]
pub(crate) struct Looked {
    pub(crate) connect: Vec<Installation>,
    pub(crate) forgot: Vec<GithubAccount>,
}

impl GithubLogins {
    /// What `app`'s installations as GitHub lists them, `found`, come to at `now_ms`. Only one on the app's owner
    /// counts, as [`on_its_owner`] says. Each account that holds a login for one that isn't among them, as once it was
    /// uninstalled on GitHub, has its login forgotten, as disconnecting it would, and of the rest, those to connect are
    /// answered with, as [`wanted`] says, as `by_person` asked or not. An app that's gone has none.
    pub(crate) fn installations_found(
        &self,
        app: GithubAppId,
        found: &[Installation],
        by_person: bool,
        now_ms: i64,
    ) -> Result<Looked, ApiError> {
        self.changing(now_ms, |files| {
            let Some(kept) = self.store.github_app(app)? else { return Ok(Looked::default()) };
            let found: Vec<&Installation> =
                found.iter().filter(|installation| on_its_owner(&kept, installation)).collect();
            let accounts = self.store.installation_accounts(app)?;
            let mut looked = Looked::default();
            for (account, file) in &accounts {
                let listed = |id| found.iter().any(|installation| installation.id == id);
                let (Some(installation), Some(file)) = (&account.installation, file) else { continue };
                if !listed(installation.id) {
                    info!(account = %account.id, "forgot an installation's login, as GitHub no longer lists it");
                    looked.forgot.extend(files.disconnect(file, || Ok(self.store.clear_github_login(&account.id)?))?);
                }
            }
            let on = |installation: &Installation| {
                accounts.iter().map(|(account, _)| account).find(|account| {
                    account.installation.as_ref().is_some_and(|installed| installed.on.id == installation.on.id)
                })
            };
            looked.connect = found
                .into_iter()
                .filter(|installation| wanted(on(installation), installation, by_person))
                .cloned()
                .collect();
            Ok(looked)
        })
    }

    /// Keeps `token`, made for `installation` of `app`, whose bot is `bot`, as the login of the account of the app on
    /// the same user or organization, or of a new one, at `now_ms`, while it's still [`wanted`], [`on_its_owner`], and
    /// the app is still kept, as when neither a client nor the hub connected it meanwhile. The first account the hub
    /// has a record of becomes the default, as it does however it's connected.
    pub(crate) fn connect_installation(
        &self,
        app: GithubAppId,
        bot: &GithubUser,
        installation: &Installation,
        token: InstallationToken,
        by_person: bool,
        now_ms: i64,
    ) -> Result<Option<GithubAccountChange>, ApiError> {
        self.changing(now_ms, |files| {
            let Some(kept) = self.store.github_app(app)? else { return Ok(None) };
            let account = self.store.installed_on(app, installation.on.id)?;
            if !on_its_owner(&kept, installation) || !wanted(account.as_ref(), installation, by_person) {
                return Ok(None);
            }
            let new_id = GithubAccountId::from(uuid::Uuid::new_v4().to_string());
            let installed = GithubInstallation { app, id: installation.id, on: installation.on.clone() };
            let (account, settings) = files.connect(&GithubLogin::Installation(token), |file| {
                let how = GithubLoginKind::App;
                let connecting = GithubConnecting {
                    account: &new_id,
                    user: bot,
                    how,
                    file,
                    connected_at_ms: now_ms,
                    ends_at_ms: None,
                    installation: None,
                };
                let GithubConnected { account, replaced, settings } =
                    self.store.connect_github_installation(&connecting, &installed)?;
                Ok(((account, settings), replaced))
            })?;
            Ok(Some(GithubAccountChange { account, changed: true, settings }))
        })?
        .map(|change| change.settled(&self.store))
        .transpose()
    }

    /// Each installation whose token has less than [`MINTED_WITH_LEFT`] left at `now_ms`, or has ended, with its app's
    /// key, under the lock `files` holds. One whose app's key can't be read is said and left, so it blocks once its
    /// token ends, saying it's being renewed. *Unconfirmed:* every installation's, not only the default's.
    pub(super) fn installations_due(
        &self,
        files: &Locked<'_, GithubLogin>,
        now_ms: i64,
    ) -> Result<Vec<MintDue>, ApiError> {
        let minted_before = later_ms(now_ms, MINTED_WITH_LEFT);
        let mut keys: HashMap<GithubAppId, Option<GithubAppKey>> = HashMap::new();
        let mut due = Vec::new();
        for (account, file) in self.store.held_github_installations()? {
            let Some(installation) = account.installation else { continue };
            let Ok(Stored::Secret(GithubLogin::Installation(held))) = files.read(&file) else { continue };
            if held.token_ends_at_ms >= minted_before {
                continue;
            }
            let key = keys.entry(installation.app).or_insert_with(|| {
                self.app_key_in(files, installation.app)
                    .inspect_err(|e| warn!(app = %installation.app, "can't make installations' tokens: {}", e.message))
                    .ok()
                    .map(|(_, key)| key)
            });
            if let Some(key) = key {
                let (app, installation) = (installation.app, installation.id);
                due.push(MintDue { account: account.id, app, installation, key: key.clone() });
            }
        }
        Ok(due)
    }

    /// Keeps what making `due`'s token came to, at `minted_at_ms`, while its account holds a login for `due`'s
    /// installation still: the token in the login's file, in place, which changes nothing on the account's record, or,
    /// when GitHub has no such installation any more, its login forgotten, as disconnecting it would. A token that
    /// can't be kept is said, and left for the next check, since the last works until it ends. Then what workers are
    /// sent is worked out again, so a default's new token reaches every worker. Answers with the account's record when
    /// that changed it.
    pub(crate) fn minted(
        &self,
        due: &MintDue,
        minted: Minted,
        minted_at_ms: i64,
    ) -> Result<Option<GithubAccountChange>, ApiError> {
        self.changing(minted_at_ms, |files| {
            let Some(account) = self.store.github_account(&due.account)? else { return Ok(None) };
            let installed = account.installation.is_some_and(|installation| installation.id == due.installation);
            let Some(file) = self.store.github_login_file(&due.account)?.filter(|_| installed) else { return Ok(None) };
            match minted {
                Minted::Token { token, ends_at_ms } => {
                    let made = GithubLogin::Installation(InstallationToken { token, token_ends_at_ms: ends_at_ms });
                    if let Err(e) = files.replace(&file, &made) {
                        let why = "couldn't keep an installation's new token, so the next check makes another";
                        warn!(account = %due.account, "{why}: {e}");
                    }
                    Ok(None)
                }
                Minted::Gone => {
                    info!(account = %due.account, "forgot an installation's login, as GitHub has no such installation");
                    let forgot = files.disconnect(&file, || Ok(self.store.clear_github_login(&due.account)?))?;
                    Ok(forgot.map(|account| GithubAccountChange { account, changed: true, settings: None }))
                }
            }
        })
    }
}

/// Whether to connect `installation`, as `account`, the account of its app on the same user or organization, stands:
/// one with none, or that's another installation, as when the app was installed there again where it had been removed,
/// is connected, and one that was disconnected only when the person asks, since disconnecting it was theirs. One that
/// holds a login for it already isn't. *Unconfirmed:* the checks leaving one the person disconnected, and one
/// installed again connecting in place.
fn wanted(account: Option<&GithubAccount>, installation: &Installation, by_person: bool) -> bool {
    let Some(account) = account else { return true };
    if account.installation.as_ref().is_none_or(|installed| installed.id != installation.id) {
        return true;
    }
    account.held_login.is_none() && by_person
}

/// Whether `installation` is on `app`'s owner, the user or organization GitHub made it under. An app of your own is
/// private, as the owner decided, so only its owner can install it, and that's its one installation. One elsewhere,
/// as once the person has made the app public on GitHub, is someone else's, which the hub doesn't connect, so no one
/// but the owner has agents act as the app. *Unconfirmed:* leaving out one elsewhere rather than connecting it.
fn on_its_owner(app: &GithubApp, installation: &Installation) -> bool {
    installation.on.id == app.owner.id
}
