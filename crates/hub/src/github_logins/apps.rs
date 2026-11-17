//! Keeping the key of an app of your own, as GitHub made it, reading it to ask GitHub as the app, and forgetting it.

use protocol::client::github_apps::GithubManifestCode;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GithubAccount, GithubApp, GithubAppId, GithubUser, Rev};

use super::{GithubLogin, GithubLogins};
use crate::error::internal;
use crate::github::{GithubAppKey, MadeApp};
use crate::login_files::{Locked, Stored};
use crate::store::{GithubAppMaking, StoreError};

/// An app's record after keeping it, and whether that changed it, so the hub announces only what did.
pub(crate) struct AppChange {
    pub(crate) app: GithubApp,
    pub(crate) changed: bool,
}

/// What removing an app came to: its record as it last was, the revision of its removal, and the record of each of
/// its installations' accounts whose login that forgot.
pub(crate) struct AppRemoved {
    pub(crate) app: GithubApp,
    pub(crate) rev: Rev,
    pub(crate) accounts: Vec<GithubAccount>,
}

impl GithubLogins {
    /// The app GitHub made from `code`, if the hub keeps one, which making it again answers with.
    pub(crate) fn made_from(&self, code: &GithubManifestCode) -> Result<Option<GithubApp>, ApiError> {
        Ok(self.store.github_app_made_from(code)?)
    }

    /// Keeps `made`'s key, which GitHub made from `code`, in a file of its own, and then its record, as
    /// [`Locked::connect`] says, unless an app from that code, or of its number, is kept already, which is let be. It
    /// changes no account's login, so what workers are sent stays as it is.
    pub(crate) fn keep_app(&self, made: &MadeApp, code: &GithubManifestCode) -> Result<AppChange, ApiError> {
        let files = self.files.lock();
        if let Some(app) = self.store.github_app_made_from(code)?.or(self.store.github_app(made.id)?) {
            return Ok(AppChange { app, changed: false });
        }
        let app = files.connect(&GithubLogin::App(made.key.clone()), |file| {
            let MadeApp { id, slug, name, owner, key: _ } = made;
            let making = GithubAppMaking { id: *id, slug, name, owner, key: file, made_from: code };
            Ok((self.store.make_github_app(&making)?, None))
        })?;
        Ok(AppChange { app, changed: true })
    }

    /// The app `id`'s record and its key, to ask GitHub as the app. Refused with `NotFound` when the hub has no record
    /// of it, and with `Conflict` when its key is gone, or in a form only a newer build reads.
    pub(crate) fn app_key(&self, id: GithubAppId) -> Result<(GithubApp, GithubAppKey), ApiError> {
        self.app_key_in(&self.files.lock(), id)
    }

    /// The app `id`'s record and its key, as [`GithubLogins::app_key`] says, under the lock `files` holds.
    pub(super) fn app_key_in(
        &self,
        files: &Locked<'_, GithubLogin>,
        id: GithubAppId,
    ) -> Result<(GithubApp, GithubAppKey), ApiError> {
        let (app, file) = self.store.github_app_key(id)?;
        let read = file.map_or(Ok(Stored::Nothing), |file| files.read(&file));
        let refused = |why: &str| Err(ApiError::new(ErrorCode::Conflict, format!("{why} the key of {}", app.slug)));
        match read {
            Ok(Stored::Secret(GithubLogin::App(key))) => Ok((app, key)),
            Ok(Stored::Newer) => refused("only a newer Roundhouse can read"),
            Ok(Stored::Secret(_) | Stored::Nothing) => refused("Roundhouse can't find"),
            Err(e) => Err(internal(format!("couldn't read the key of {}: {e}", app.slug))),
        }
    }

    /// Says the app `id`'s bot is `bot`, as GitHub said, and answers with its record when that changed it.
    pub(crate) fn app_bot(&self, id: GithubAppId, bot: &GithubUser) -> Result<Option<GithubApp>, ApiError> {
        Ok(self.store.set_github_app_bot(id, bot)?)
    }

    /// Has the hub forget the app `id`, at `now_ms`, as `GithubAppRemove` says: the login of each of its
    /// installations, as disconnecting each does, then its key, and then its record, so whatever stops the hub between
    /// leaves a record whose key is gone, which tidying removes as the hub next starts, and never a token or a key
    /// that was to be forgotten. Then what workers are sent is worked out again, so a default that was one of them
    /// blocks. Refused with `NotFound` when the hub has no record of it.
    pub(crate) fn remove_app(&self, id: GithubAppId, now_ms: i64) -> Result<AppRemoved, ApiError> {
        self.changing(now_ms, |files| {
            let (app, key) = self.store.github_app_key(id)?;
            let mut accounts = Vec::new();
            for (account, file) in self.store.installation_accounts(id)? {
                let Some(file) = file else { continue };
                accounts.extend(files.disconnect(&file, || Ok(self.store.clear_github_login(&account.id)?))?);
            }
            let removed = match key {
                Some(key) => files.disconnect(&key, || Ok(self.store.remove_github_app(id)?))?,
                None => self.store.remove_github_app(id)?,
            };
            let rev = removed.ok_or_else(|| StoreError::NotFound { kind: "app of your own", id: id.to_string() })?;
            Ok(AppRemoved { app, rev, accounts })
        })
    }
}
