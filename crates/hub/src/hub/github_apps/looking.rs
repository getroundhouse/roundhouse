//! Looking for where an app of your own is installed, connecting each installation with a token made from the app's
//! key, and making each installation's token again before its last ends. GitHub is asked between turns at the logins'
//! lock, never under it. What it answers is the `github` crate's to read, and what's kept of it, and in what order, is
//! `crate::github_logins`'s.

use std::sync::Arc;

use protocol::client::{ApiError, HubNotice};
use protocol::model::{GithubApp, GithubAppId};
use tracing::{error, warn};

use super::super::{Hub, off_thread};
use crate::github_logins::{GithubAccountChange, InstallationToken, Looked, MintDue};
use crate::store::now_ms;
use github::Minted;

impl Hub {
    /// Looks for where the app `id` is installed, and connects each installation [`GithubLogins::installations_found`]
    /// says to, as `by_person` asked or as the hub does by itself, telling every client of each record that changed,
    /// and answers with the app's record. GitHub is asked first for the app's bot, until it has said, then for its
    /// installations, then for a token for each to connect, each within its time limit. Refused with what GitHub's
    /// first failure was, which leaves what was connected before it.
    ///
    /// [`GithubLogins::installations_found`]: crate::github_logins::GithubLogins::installations_found
    pub(super) async fn look_at_app(&self, id: GithubAppId, by_person: bool) -> Result<GithubApp, ApiError> {
        let logins = Arc::clone(&self.github_logins);
        let (mut app, key) = off_thread(move || logins.app_key(id)).await?;
        let bot = if let Some(bot) = app.bot.clone() {
            bot
        } else {
            let bot = self.github.bot(&app.slug).await?;
            let (logins, told) = (Arc::clone(&self.github_logins), bot.clone());
            if let Some(changed) = off_thread(move || logins.app_bot(id, &told)).await? {
                self.notify(HubNotice::GithubApp(changed.clone()));
                app = changed;
            }
            bot
        };
        let found = self.github.installations(id, &key).await?;
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
            let (logins, told, connected_at_ms) = (Arc::clone(&self.github_logins), bot.clone(), now_ms());
            let connecting =
                move || logins.connect_installation(id, &told, &installation, token, by_person, connected_at_ms);
            if let Some(change) = off_thread(connecting).await? {
                let change = self.github_user_connected(change, bot.clone(), None, connected_at_ms).await;
                self.announce_github_account(change);
            }
        }
        Ok(app)
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
                let why = "couldn't make an installation's token, so the next check will";
                warn!(account = %due.account, "{why}: {}", e.message);
                return;
            }
        };
        let (logins, minted_at_ms) = (Arc::clone(&self.github_logins), now_ms());
        match off_thread(move || logins.minted(&due, minted, minted_at_ms)).await {
            Ok(Some(change)) => {
                self.announce_github_account(change);
            }
            Ok(None) => {}
            Err(e) => error!("couldn't keep an installation's new token: {}", e.message),
        }
    }

    /// Looks for where the app `id` is installed, as the hub does by itself as it checks its GitHub logins, saying in
    /// the log what failed, for the next check to try again. *Unconfirmed:* looking at every check, as well as when a
    /// client asks.
    pub(in super::super) async fn look_by_itself(&self, id: GithubAppId) {
        if let Err(e) = self.look_at_app(id, false).await {
            warn!(app = %id, "couldn't look for where an app of your own is installed: {}", e.message);
        }
    }
}
