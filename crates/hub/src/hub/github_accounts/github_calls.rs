use super::super::{Hub, off_thread};
use crate::github_logins::{Connect, GithubAccountChange};
use crate::store::now_ms;
use protocol::client::github_accounts::{GithubAccountConnect, GithubAccountSetDefault};
use protocol::client::{ApiError, HubNotice};
use protocol::model::{GithubAccount, GithubAccountId, HubSettings};
use std::sync::Arc;
use tracing::error;

impl Hub {
    /// Has the hub hold a GitHub token, as `GithubAccountConnect` says, and tells every client what that changed of the
    /// account's record, and of the settings when it made the account the default. GitHub is asked whose the token is
    /// between two turns at the logins' lock, never under it, so a GitHub that doesn't answer holds up only this.
    pub(crate) async fn github_account_connect(&self, asked: GithubAccountConnect) -> Result<GithubAccount, ApiError> {
        let connect = Arc::new(Connect::new(asked, now_ms())?);
        let (logins, before) = (Arc::clone(&self.github_logins), Arc::clone(&connect));
        if let Some(held) = off_thread(move || logins.held(&before)).await? {
            return Ok(held.account);
        }
        let owner = self.github.token_owner(connect.token()).await?;
        let logins = Arc::clone(&self.github_logins);
        let change = off_thread(move || logins.connect(&connect, &owner)).await?;
        Ok(self.announce_github_account(change))
    }

    /// Has the hub forget the login it holds for the GitHub account, as `GithubAccountDisconnect` says, and tells every
    /// client when that changed the account's record.
    pub(crate) async fn github_account_disconnect(&self, id: GithubAccountId) -> Result<GithubAccount, ApiError> {
        let logins = Arc::clone(&self.github_logins);
        let change = off_thread(move || logins.disconnect(&id)).await?;
        Ok(self.announce_github_account(change))
    }

    /// Chooses the default GitHub account, as `GithubAccountSetDefault` says, and tells every client of the settings.
    pub(crate) async fn github_account_set_default(
        &self,
        asked: GithubAccountSetDefault,
    ) -> Result<HubSettings, ApiError> {
        let (logins, chosen) = (Arc::clone(&self.github_logins), asked.account);
        let settings = off_thread(move || logins.set_default(chosen.as_ref(), now_ms())).await?;
        self.notify(HubNotice::HubSettings(settings.clone()));
        Ok(settings)
    }

    /// Makes the GitHub logins' files and the GitHub accounts' rows agree, as the hub starts, as
    /// [`Hub::tidy_logins`] says.
    pub(in super::super) async fn tidy_github_logins(&self) {
        let logins = Arc::clone(&self.github_logins);
        if let Err(e) = off_thread(move || logins.tidy()).await {
            error!("couldn't make the GitHub logins' files and the GitHub accounts agree: {}", e.message);
        }
    }

    fn announce_github_account(&self, change: GithubAccountChange) -> GithubAccount {
        let GithubAccountChange { account, changed, settings } = change;
        if changed {
            self.notify(HubNotice::GithubAccount(account.clone()));
        }
        if let Some(settings) = settings {
            self.notify(HubNotice::HubSettings(settings));
        }
        account
    }
}
