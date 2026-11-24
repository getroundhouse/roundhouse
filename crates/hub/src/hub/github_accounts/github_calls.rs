use super::super::{Hub, off_thread};
use crate::github_logins::{Connect, Disconnected, GithubAccountChange};
use crate::store::GithubRemoved;
use crate::store::now_ms;
use protocol::client::github_accounts::{GithubAccountConnect, GithubAccountSetDefault};
use protocol::client::{ApiError, HubNotice};
use protocol::model::{GithubAccount, GithubAccountId, HubSettings, RecordKey};
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
            return Ok(self.announce_github_account(held));
        }
        let owner = Arc::new(self.github.token_owner(connect.token()).await?);
        let (logins, connecting, told) = (Arc::clone(&self.github_logins), Arc::clone(&connect), Arc::clone(&owner));
        let change = off_thread(move || logins.connect(&connecting, &told)).await?;
        let (user, profile) = (owner.user.clone(), owner.profile.clone());
        let change = self.github_user_connected(change, user, Some(profile), connect.connected_at_ms()).await;
        Ok(self.announce_github_account(change))
    }

    /// Has the hub forget the login it holds for the GitHub account, and the account with it but for an app's
    /// installation, as `GithubAccountDisconnect` says, and tells every client what that changed: the installation's
    /// record, or the account's removal, and the settings when it was the default. A removed account's reply is its
    /// record as it last was, as a removal's always is.
    pub(crate) async fn github_account_disconnect(&self, id: GithubAccountId) -> Result<GithubAccount, ApiError> {
        let logins = Arc::clone(&self.github_logins);
        match off_thread(move || logins.disconnect(&id, now_ms())).await? {
            Disconnected::Kept(change) => Ok(self.announce_github_account(change)),
            Disconnected::Removed(GithubRemoved { mut account, rev, settings }) => {
                // Named as it last was, before what GitHub calls its user is forgotten with it.
                self.fill_old_forms(&mut account);
                self.notify(HubNotice::Removed { record: RecordKey::GithubAccount { id: account.id.clone() }, rev });
                if let Some(settings) = settings {
                    self.notify(HubNotice::HubSettings(settings));
                }
                self.github_account_observations_gone(&account.id).await;
                Ok(account)
            }
        }
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
    /// [`Hub::tidy_logins`] says. Then reads `gh` while an account holds its login, since its token is kept only in
    /// memory, so the agents an earlier hub left aren't blocked until the first check.
    pub(in super::super) async fn tidy_github_logins(&self) {
        let logins = Arc::clone(&self.github_logins);
        if let Err(e) = off_thread(move || logins.tidy(now_ms())).await {
            error!("couldn't make the GitHub logins' files and the GitHub accounts agree: {}", e.message);
        }
        // Of each account tidying removed.
        self.observations_of_no_github_account_gone().await;
        let logins = Arc::clone(&self.github_logins);
        match off_thread(move || logins.holds_gh()).await {
            Ok(true) => self.read_gh().await,
            Ok(false) => {}
            Err(e) => error!("couldn't tell whether an account holds gh's login: {}", e.message),
        }
    }

    /// Tells every client what changed of a GitHub account's record, and of the settings when it changed the default,
    /// and returns the record, named as the hub last heard GitHub call its user.
    pub(in super::super) fn announce_github_account(&self, change: GithubAccountChange) -> GithubAccount {
        let GithubAccountChange { mut account, changed, settings } = change;
        self.fill_old_forms(&mut account);
        if changed {
            self.notify(HubNotice::GithubAccount(account.clone()));
        }
        if let Some(settings) = settings {
            self.notify(HubNotice::HubSettings(settings));
        }
        account
    }
}
