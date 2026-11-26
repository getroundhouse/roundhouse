//! Looking for where an app of your own is installed, connecting each installation with a token made from the app's
//! key, and making each installation's token again before its last ends. GitHub is asked between turns at the logins'
//! lock, never under it. What it answers is the `github` crate's to read, and what's kept of it, and in what order, is
//! `crate::github_logins`'s.

use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GithubApp, GithubAppId, GithubUser};
use tracing::{error, warn};

use super::super::{Hub, off_thread};
use crate::github_logins::{GithubAccountChange, InstallationToken, Looked, MintDue, PROFILE_READ_EVERY};
use crate::hub::{GithubAppBot, GithubAppSaid, InstalledOn};
use crate::store::{StoreError, now_ms};
use github::{GithubAppKey, Installation, Minted};

impl Hub {
    /// Looks for where the app `id` is installed, and connects each installation [`GithubLogins::installations_found`]
    /// says to, as `by_person` asked or as the hub does by itself, telling every client of each record that changed,
    /// and answers with the app's record. GitHub is asked first what it says of the app, as [`Hub::read_app`] says,
    /// then for the app's bot, until it has said, then for its installations, whose user or organization each is on
    /// the hub keeps, then for a token for each to connect, each within its time limit. Refused with what GitHub's
    /// first failure was, which leaves what was connected before it, and when the app's key is gone or can't be read.
    ///
    /// [`GithubLogins::installations_found`]: crate::github_logins::GithubLogins::installations_found
    pub(super) async fn look_at_app(&self, id: GithubAppId, by_person: bool) -> Result<GithubApp, ApiError> {
        let logins = Arc::clone(&self.github_logins);
        let (_, key) = off_thread(move || logins.app_key(id)).await?;
        self.read_app(id, &key).await?;
        let bot = self.app_bot(id).await?;
        let found = self.github.installations(id, &key).await?;
        self.installations_named(id, &found).await;
        let logins = Arc::clone(&self.github_logins);
        let Looked { connect, forgot } =
            off_thread(move || logins.installations_found(id, &found, by_person, now_ms())).await?;
        for account in forgot {
            self.announce_github_account(GithubAccountChange { account, changed: true, settings: None });
        }
        for installation in connect {
            let Minted::Token { token, ends_at_ms } = self.github.installation_token(id, &key, installation.id).await?
            else {
                continue;
            };
            let token = InstallationToken { token, token_ends_at_ms: ends_at_ms };
            let (logins, told, connected_at_ms, on) =
                (Arc::clone(&self.github_logins), bot.clone(), now_ms(), installation.on.name.clone());
            let connecting =
                move || logins.connect_installation(id, &told, &installation, token, by_person, connected_at_ms);
            if let Some(change) = off_thread(connecting).await? {
                let account = change.account.id.clone();
                let change = self.github_user_connected(change, bot.clone(), None, connected_at_ms).await;
                self.observation_read::<InstalledOn>(account, on, connected_at_ms).await;
                self.workers_sent_again(id).await;
                self.announce_github_account(change);
            }
        }
        self.app_record(id).await
    }

    /// Asks GitHub, as the app `id`, with its `key`, what it says of the app, its slug, name and owner, and keeps it, at
    /// once when the hub doesn't know it, as on the first start after it stopped keeping it in its database, and a day
    /// after it last asked, as it does a GitHub account's user, so an app renamed or moved to another owner on GitHub
    /// shows as it is. A failure is only said while the hub knows what GitHub said before, which stands, and refuses
    /// the look while it knows nothing, since it can't tell which installation is on the app's owner, nor its bot.
    async fn read_app(&self, id: GithubAppId, key: &GithubAppKey) -> Result<(), ApiError> {
        let asked_at_ms = now_ms();
        let every = i64::try_from(PROFILE_READ_EVERY.as_millis()).unwrap_or(i64::MAX);
        let read_at_ms = self.observations.read_at::<GithubAppSaid>(&id);
        if read_at_ms.is_some_and(|read_at_ms| read_at_ms > asked_at_ms.saturating_sub(every)) {
            return Ok(());
        }
        match self.github.app(id, key).await {
            Ok(said) => self.observation_read::<GithubAppSaid>(id, said, asked_at_ms).await.result(),
            Err(e) if read_at_ms.is_some() => {
                warn!(
                    name: "github.app_unasked",
                    app = %id,
                    error = e.message,
                    "Couldn't ask GitHub what it says of an app, so what it said before stands"
                );
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// The app `id`'s bot, as the hub holds it, or else as GitHub says it, asked as nobody by the app's slug, which it
    /// keeps. Refused while the hub knows neither, as when GitHub hasn't said what it calls the app.
    async fn app_bot(&self, id: GithubAppId) -> Result<GithubUser, ApiError> {
        if let Some(bot) = self.observations.value_of::<GithubAppBot>(&id) {
            return Ok(bot);
        }
        let Some(said) = self.observations.value_of::<GithubAppSaid>(&id) else {
            return Err(ApiError::new(ErrorCode::Unavailable, "GitHub hasn't said what it calls the app yet"));
        };
        let asked_at_ms = now_ms();
        let bot = self.github.bot(&said.slug).await?;
        self.observation_read::<GithubAppBot>(id, bot.clone(), asked_at_ms).await.result()?;
        Ok(bot)
    }

    /// Keeps what GitHub calls the user or organization each installation of the app `id` that `found` lists is on, for
    /// the GitHub account that's that installation, by the number of what it's on, which links the two.
    async fn installations_named(&self, id: GithubAppId, found: &[Installation]) {
        let accounts = match self.blocking(move |store| store.installation_accounts(id)).await {
            Ok(accounts) => accounts,
            Err(e) => {
                return warn!(
                    name: "github.installation_accounts_unread",
                    app = %id,
                    error = e.message,
                    "Couldn't read an app's installations' accounts"
                );
            }
        };
        let read_at_ms = now_ms();
        for (account, _) in accounts {
            let Some(installed) = &account.installation else { continue };
            let Some(listed) = found.iter().find(|listed| listed.on.id == installed.on.id) else { continue };
            self.observation_read::<InstalledOn>(account.id, listed.on.name.clone(), read_at_ms).await;
        }
        self.workers_sent_again(id).await;
    }

    /// Works out what workers are sent again, once the hub has kept where an installation of the app `id` is, since what
    /// they're sent, and the check of reach, say where the default's installation is.
    async fn workers_sent_again(&self, id: GithubAppId) {
        let logins = Arc::clone(&self.github_logins);
        if let Err(e) = off_thread(move || logins.heard_of(now_ms())).await {
            warn!(
                name: "github.sent_not_worked_out",
                app = %id,
                error = e.message,
                "Couldn't work out what workers are sent again"
            );
        }
    }

    /// The app `id`'s record. Refused once it's gone.
    pub(super) async fn app_record(&self, id: GithubAppId) -> Result<GithubApp, ApiError> {
        let found = self.blocking(move |store| store.github_app(id)).await?;
        Ok(found.ok_or_else(|| StoreError::NotFound { kind: "app of your own", id: id.to_string() })?)
    }

    /// Makes `due`'s installation a new token, within GitHub's time limit, and has
    /// [`GithubLogins::minted`](crate::github_logins::GithubLogins::minted) keep it, or forget the installation's login
    /// when GitHub has no such installation, telling every client of what that changed. One GitHub doesn't make is
    /// said in the log and left for the next check, so a default whose token ends meanwhile blocks, saying the next is
    /// being made.
    pub(in super::super) async fn mint(&self, due: MintDue) {
        let minted = match self.github.installation_token(due.app, &due.key, due.installation).await {
            Ok(minted) => minted,
            Err(e) => {
                warn!(
                    name: "github.installation_token_unmade",
                    account = %due.account,
                    error = e.message,
                    "Couldn't make an installation's token, so the next check will"
                );
                return;
            }
        };
        let (logins, minted_at_ms) = (Arc::clone(&self.github_logins), now_ms());
        match off_thread(move || logins.minted(&due, minted, minted_at_ms)).await {
            Ok(Some(change)) => {
                self.announce_github_account(change);
            }
            Ok(None) => {}
            Err(e) => {
                error!(
                    name: "github.installation_token_unkept",
                    error = e.message,
                    "Couldn't keep an installation's new token"
                );
            }
        }
    }

    /// Looks for where the app `id` is installed, as the hub does by itself as it checks its GitHub logins, saying in
    /// the log what failed, for the next check to try again. *Unconfirmed:* looking at every check, as well as when a
    /// client asks.
    pub(in super::super) async fn look_by_itself(&self, id: GithubAppId) {
        if let Err(e) = self.look_at_app(id, false).await {
            warn!(
                name: "github.app_not_looked_for",
                app = %id,
                error = e.message,
                "Couldn't look for where an app of your own is installed"
            );
        }
    }
}
