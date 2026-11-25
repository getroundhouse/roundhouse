use std::sync::Arc;

use protocol::client::github_accounts::GithubSignInStart;
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{GithubSignIn, GithubSignInId, GithubSignInStatus, RecordKey};
use tracing::error;

use super::super::{Hub, off_thread};
use crate::github_logins::{SignInChange, SignInEnd};
use crate::jobs::{JobKind, NotStarted};
use crate::store::{StartedSignIn, later_ms, now_ms};

impl Hub {
    /// Starts a sign-in with GitHub, as `GithubSignInStart` says, and tells every client of its record, and of the
    /// removal of each that had ended. GitHub is asked for its code before anything is kept, so one it won't give
    /// changes nothing, and then a job of the hub's asks after it, as [`Hub::wait_for_sign_in`] says.
    pub(crate) async fn github_sign_in_start(
        self: &Arc<Self>,
        asked: GithubSignInStart,
    ) -> Result<GithubSignIn, ApiError> {
        let GithubSignInStart { id } = asked;
        let known = id.clone();
        if let Some(sign_in) = self.blocking(move |store| store.github_sign_in(&known)).await? {
            return Ok(sign_in);
        }
        // Checked again as its job starts, but first here, so a sign-in that couldn't wait asks GitHub for no code.
        if let Err(not) = self.sign_ins.room_for(JobKind::WaitForGithubSignIn) {
            let code = if not == NotStarted::Stopped { ErrorCode::Unavailable } else { ErrorCode::Conflict };
            return Err(ApiError::new(code, why_not(not)));
        }
        let code = self.github.device_code().await?;
        let code_ends_at_ms = later_ms(now_ms(), code.lasts);
        let waiting = GithubSignInStatus::Waiting {
            code: code.user_code.clone(),
            address: code.address.clone(),
            code_ends_at_ms,
        };
        let starting = id.clone();
        let started = self.blocking(move |store| store.start_github_sign_in(&starting, &waiting)).await?;
        let StartedSignIn { sign_in, made, removed } = started;
        for (gone, rev) in removed {
            self.notify(HubNotice::Removed { record: RecordKey::GithubSignIn { id: gone }, rev });
        }
        // Another start of the same sign-in made it meanwhile, and its code is the one the person is shown.
        if !made {
            return Ok(sign_in);
        }
        self.notify(HubNotice::GithubSignIn(sign_in.clone()));
        if let Err(not) = self.wait_for_sign_in(id.clone(), code) {
            // As many started meanwhile, or the hub is stopping, so nothing will ask after it.
            let change = self.end_sign_in(id, SignInEnd::Failed(why_not(not).to_string())).await?;
            return Ok(self.announce_sign_in(change));
        }
        Ok(sign_in)
    }

    /// Cancels a sign-in, as `GithubSignInCancel` says, so its job stops asking GitHub, and tells every client when
    /// that changed its record. Its record is ended first, under the logins' lock, so a token GitHub gives meanwhile
    /// is never kept.
    pub(crate) async fn github_sign_in_cancel(&self, id: GithubSignInId) -> Result<GithubSignIn, ApiError> {
        let change = self.end_sign_in(id.clone(), SignInEnd::Cancelled).await?;
        if change.changed {
            self.sign_ins.call_off(&id);
        }
        Ok(self.announce_sign_in(change))
    }

    /// Ends the sign-in `id` as `end` says, unless it has ended, as `GithubLogins::end_sign_in` says.
    pub(super) async fn end_sign_in(&self, id: GithubSignInId, end: SignInEnd) -> Result<SignInChange, ApiError> {
        let logins = Arc::clone(&self.github_logins);
        off_thread(move || logins.end_sign_in(&id, end)).await
    }

    /// The same, for the sign-in's own job, which no request waits on, so what fails is only said, and what changed
    /// told every client.
    pub(super) async fn end_sign_in_alone(&self, id: GithubSignInId, end: SignInEnd) {
        match self.end_sign_in(id.clone(), end).await {
            Ok(change) => {
                self.announce_sign_in(change);
            }
            Err(e) => {
                error!(
                    name: "github.sign_in_unrecorded",
                    sign_in = %id,
                    error = e.message,
                    "Couldn't record how a sign-in with GitHub ended"
                );
            }
        }
    }

    /// Tells every client what changed of a sign-in's record, and first of the account approving it made or renewed,
    /// so a client that hears it's approved holds its account.
    pub(super) fn announce_sign_in(&self, change: SignInChange) -> GithubSignIn {
        let SignInChange { sign_in, changed, account } = change;
        if let Some(account) = account {
            self.announce_github_account(account);
        }
        if changed {
            self.notify(HubNotice::GithubSignIn(sign_in.clone()));
        }
        sign_in
    }
}

/// Why a sign-in couldn't wait for GitHub, as a person reads it.
fn why_not(not: NotStarted) -> &'static str {
    match not {
        NotStarted::KindBusy => "as many sign-ins with GitHub as may wait at once are waiting; finish or cancel one",
        NotStarted::KeyBusy => "a sign-in with GitHub of the same ID is waiting already",
        NotStarted::Stopped => "the hub is stopping",
    }
}
