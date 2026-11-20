//! Reading again, as the hub checks by the wall clock, who each token's and sign-in's account is on GitHub, its name
//! and public email, which its commit identity is made from: which are due, and keeping what GitHub said. The hub asks
//! GitHub between the two, never under the logins' lock.

use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GithubAccountId, GithubToken, GithubUserId};
use tracing::warn;

use super::{Due, GithubAccountChange, GithubLogin, GithubLogins};
use crate::login_files::{Locked, Stored};
use github::TokenOwner;

/// How long after the hub last asked GitHub who an account is it asks again, so a name or an email changed on GitHub
/// reaches workers within a day, at a call a day for each account. One never asked, as an account an older hub
/// connected, is asked at the next check, so within a minute of the hub starting. *Unconfirmed.*
pub(crate) const PROFILE_READ_EVERY: Duration = Duration::from_hours(24);

/// An account to ask GitHub who it is, as a check found it: its record's user, to check GitHub's answer is the same
/// user, and its login's token to ask with, as its file held it then.
#[derive(Debug)]
pub(crate) struct ProfileDue {
    pub(crate) account: GithubAccountId,
    pub(crate) user: GithubUserId,
    pub(crate) token: GithubToken,
}

impl GithubLogins {
    /// Each account whose user's profile the hub last asked GitHub for [`PROFILE_READ_EVERY`] or longer before `now_ms`,
    /// or never, whose login works by its record and holds a token, under the lock `files` holds. One being renewed, among
    /// `renewing`, is left for the next check, which asks with the token renewing gives, since GitHub revokes the
    /// token its file holds now as it gives the next.
    pub(super) fn profiles_due(
        &self,
        files: &Locked<'_, GithubLogin>,
        renewing: &[Due],
        now_ms: i64,
    ) -> Result<Vec<ProfileDue>, ApiError> {
        let every = i64::try_from(PROFILE_READ_EVERY.as_millis()).unwrap_or(i64::MAX);
        let mut due = Vec::new();
        for (account, file) in self.store.github_profiles_due(now_ms.saturating_sub(every))? {
            let works = account.held_login.is_some_and(|held| held.works_at(now_ms));
            if !works || renewing.iter().any(|renewing| renewing.account == account.id) {
                continue;
            }
            // One whose file holds no token, one in a form only a newer build reads, or one that can't be read, which
            // blocks as it is while it's the default's, as `token_of` says.
            let Ok(Stored::Secret(login)) = files.read(&file) else { continue };
            let token = match login {
                GithubLogin::Gh { user } => self.gh_token(user, now_ms).ok(),
                login @ (GithubLogin::Token(_)
                | GithubLogin::SignIn(_)
                | GithubLogin::Installation(_)
                | GithubLogin::App(_)) => login.token().cloned(),
            };
            let Some(token) = token else { continue };
            due.push(ProfileDue { account: account.id, user: account.user.id, token });
        }
        Ok(due)
    }

    /// Keeps what asking GitHub who `due`'s account is came to, as GitHub was asked at `asked_at_ms`: the user's name
    /// and public email, and its username, which answers with the account's record when that changed it. GitHub naming
    /// another user than the account's is let go of, and so is an answer asked before one the hub has kept, as when the
    /// account was connected again meanwhile. GitHub refusing the token is kept as an ask, so it's asked again only a
    /// day on, and anything else, such as no answer, at the next check. Then what workers are sent is worked out again,
    /// as every change to the logins has it, so a default's new name or email reaches every worker.
    pub(crate) fn profile_read(
        &self,
        due: &ProfileDue,
        read: Result<TokenOwner, ApiError>,
        asked_at_ms: i64,
    ) -> Result<Option<GithubAccountChange>, ApiError> {
        self.changing(asked_at_ms, |_| {
            let owner = match read {
                Ok(owner) if owner.user.id == due.user => owner,
                Ok(owner) => {
                    warn!(account = %due.account, "GitHub said its token is another user's, {}", owner.user.name);
                    return Ok(None);
                }
                Err(e) if e.code == ErrorCode::InvalidRequest => {
                    warn!(account = %due.account, "GitHub didn't take its token, asked who it is: {}", e.message);
                    self.store.github_profile_refused(&due.account, asked_at_ms)?;
                    return Ok(None);
                }
                Err(e) => {
                    warn!(account = %due.account, "couldn't ask GitHub who it is, so the next check will: {}", e.message);
                    return Ok(None);
                }
            };
            let renamed = self.store.read_github_profile(&due.account, &owner.user, &owner.profile, asked_at_ms)?;
            Ok(renamed.map(|account| GithubAccountChange { account, changed: true, settings: None }))
        })
    }
}
