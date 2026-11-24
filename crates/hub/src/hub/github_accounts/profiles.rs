//! Asking GitHub again who an account the check found due is, for what GitHub calls its user and its commit identity,
//! and keeping what the hub hears of who an account is, as it's read again or connected, as observations. What GitHub
//! answers is the `github` crate's to read, and which reads are due, and what an answer comes to, is
//! `crate::github_logins`'s.

use std::sync::Arc;

use protocol::client::ApiError;
use protocol::model::GithubUser;
use tracing::{error, warn};

use super::super::{Hub, off_thread};
use crate::github_logins::{GithubAccountChange, Heard, ProfileDue, ReadFrom};
use crate::hub::{GithubProfile, GithubUsername};
use crate::store::now_ms;

impl Hub {
    /// Reads who `due`'s account is: asks GitHub, within its time limit, with the token its file held as the check found
    /// it, and has `GithubLogins::profile_read` say what that comes to, or with no login, by its user's number, as
    /// `GithubLogins::nobody_read` says, or, for an app's installation, takes its app's bot as its record says it, each
    /// as of `checked_at_ms`, when the check that found it due ran, by the wall clock. Then keeps what it heard, as [`Hub::github_user_heard`] says.
    pub(super) async fn read_profile(&self, due: ProfileDue, checked_at_ms: i64) {
        let heard = match &due.from {
            ReadFrom::Github(token) => {
                let asked_at_ms = checked_at_ms;
                let read = self.github.token_owner(token).await;
                let logins = Arc::clone(&self.github_logins);
                match off_thread(move || logins.profile_read(&due, read, asked_at_ms)).await {
                    Ok(heard) => heard,
                    Err(e) => return error!("couldn't keep who GitHub says an account is: {}", e.message),
                }
            }
            ReadFrom::Nobody => {
                let asked_at_ms = checked_at_ms;
                let read = self.github.user_numbered(due.user).await;
                let logins = Arc::clone(&self.github_logins);
                match off_thread(move || logins.nobody_read(&due, read, asked_at_ms)).await {
                    Ok(heard) => heard,
                    Err(e) => return error!("couldn't keep who GitHub says an account is: {}", e.message),
                }
            }
            ReadFrom::Bot(bot) => {
                Some(Heard { account: due.account.clone(), username: bot.clone(), profile: None, at_ms: checked_at_ms })
            }
        };
        if let Some(heard) = heard
            && let Err(e) = self.github_user_heard(heard).await
        {
            error!("couldn't keep who GitHub says an account is: {}", e.message);
        }
    }

    /// Keeps that GitHub said `user` is the user of the GitHub account `change` connected at `at_ms`, with what its
    /// `profile` says, if it was asked, as connecting an account does, and returns `change` with the account's record
    /// and the settings it changed as they are once that's kept and workers are sent what it makes of the default, so
    /// the copies the hub announces are the newest, and a new default's never says it blocks for a username the hub
    /// has just heard.
    pub(in super::super) async fn github_user_connected(
        &self,
        change: GithubAccountChange,
        user: GithubUser,
        profile: Option<github::GithubProfile>,
        at_ms: i64,
    ) -> GithubAccountChange {
        let heard = Heard { account: change.account.id.clone(), username: user.name, profile, at_ms };
        if let Err(e) = self.github_user_heard(heard).await {
            warn!("couldn't keep who GitHub says a connected account is, so the next check asks: {}", e.message);
        }
        let store = Arc::clone(&self.store);
        let settling = change.clone();
        // Read again, so the copy announced is the newest, whatever changed it meanwhile.
        let settled = move || {
            let account = store.github_account(&settling.account.id)?.unwrap_or_else(|| settling.account.clone());
            let read_again = GithubAccountChange { account, ..settling };
            if read_again.settings.is_none() { Ok(read_again) } else { read_again.settled(&store) }
        };
        match off_thread(settled).await {
            Ok(settled) => settled,
            Err(e) => {
                warn!("reading the settings a connected account changed: {}", e.message);
                change
            }
        }
    }

    /// Keeps what the hub `heard` of who a GitHub account is, as observations. One heard as the account was removed is
    /// forgotten with it. Then works out what workers are sent again, since the default's is made from it, and the
    /// check of reach follows that, so it reads nothing as an account whose username it didn't know.
    async fn github_user_heard(&self, heard: Heard) -> Result<(), ApiError> {
        let Heard { account, username, profile, at_ms } = heard;
        self.observation_read::<GithubUsername>(account.clone(), username, at_ms).await?;
        if let Some(profile) = profile {
            self.observation_read::<GithubProfile>(account.clone(), profile, at_ms).await?;
        }
        let id = account.clone();
        if self.blocking(move |store| store.github_account(&id)).await?.is_none() {
            self.github_account_observations_gone(&account).await;
        }
        let logins = Arc::clone(&self.github_logins);
        off_thread(move || logins.heard_of(now_ms())).await
    }
}
