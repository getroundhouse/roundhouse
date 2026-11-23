//! Reading again, as the hub checks by the wall clock, who each GitHub account is on GitHub: for a token's, a
//! sign-in's and `gh`'s, what GitHub calls its user, and the name and public email its commit identity is made from,
//! as GitHub says them, and for an app's installation, what GitHub calls its user, which is the app's bot, as the app's
//! record says. Which are due, and what an answer comes to. The hub asks GitHub between the two, never under the
//! logins' lock, and keeps what it heard as observations, as `crate::hub::observations::GithubUsername` says.

use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GithubAccount, GithubAccountId, GithubToken, GithubUserId, GithubUsername};
use tracing::warn;

use super::{Due, GithubLogin, GithubLogins};
use crate::hub::{GithubProfile, GithubUsername as Username};
use crate::login_files::{Locked, LoginFile, Stored};
use github::TokenOwner;

/// How long after the hub last asked GitHub who an account is it asks again, so a username, a name or an email changed
/// on GitHub reaches workers within a day, at a call a day for each account. One the hub doesn't know, as every
/// account's on the first start after the hub stopped keeping them in its database, is asked at the next check, so
/// within a minute of the hub starting, or as it starts. *Unconfirmed.*
pub(crate) const PROFILE_READ_EVERY: Duration = Duration::from_hours(24);

/// An account to read who it is again, as a check found it: its record's user, to check an answer is the same user,
/// and what to read it from.
#[derive(Debug)]
pub(crate) struct ProfileDue {
    pub(crate) account: GithubAccountId,
    pub(crate) user: GithubUserId,
    pub(crate) from: ReadFrom,
}

/// What a due read of who an account is reads.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReadFrom {
    /// GitHub, with the token the account's login's file held as the check found it.
    Github(GithubToken),
    /// Nothing outside the hub, since the account is an app's installation, whose user is the app's bot, which GitHub
    /// calls this, as the app's record says.
    Bot(GithubUsername),
    /// GitHub, with no login, by the user's number, since the account's own login can't be asked with.
    Nobody,
}

/// What the hub heard of who an account's user is, to keep as observations: what GitHub calls it, and, for any but an
/// app's installation, whose bot has none, what its profile says, as GitHub said them at `at_ms`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Heard {
    pub(crate) account: GithubAccountId,
    pub(crate) username: GithubUsername,
    pub(crate) profile: Option<github::GithubProfile>,
    pub(crate) at_ms: i64,
}

impl GithubLogins {
    /// Each account whose user the hub doesn't know, or last read [`PROFILE_READ_EVERY`] or longer before `now_ms`, as
    /// [`GithubLogins::read_due`] says, under the lock `files` holds. A token's, a sign-in's or `gh`'s is read from
    /// GitHub with its token while its login works by its record and holds one. One being renewed, among `renewing`, is
    /// left for the next check, which asks with the token renewing gives, since GitHub revokes the token its file holds
    /// now as it gives the next. An app's installation is read from its app's record, while that says the app's bot.
    /// Any other, whose own login can't be asked with, as one whose token has ended, or that GitHub has refused since it
    /// was connected, or an installation whose app has been removed, is asked of GitHub with no login, by its user's
    /// number, as far as [`NobodyAsks`](super::nobody::NobodyAsks) lets it.
    pub(super) fn profiles_due(
        &self,
        files: &Locked<'_, GithubLogin>,
        renewing: &[Due],
        now_ms: i64,
    ) -> Result<Vec<ProfileDue>, ApiError> {
        let every = i64::try_from(PROFILE_READ_EVERY.as_millis()).unwrap_or(i64::MAX);
        let read_by = now_ms.saturating_sub(every);
        let mut due = Vec::new();
        for (account, file) in self.store.github_accounts_and_files()? {
            if !self.read_due(&account, read_by) {
                continue;
            }
            let (id, user) = (account.id.clone(), account.user.id);
            if let Some(installation) = &account.installation {
                let bot = self.store.github_app(installation.app)?.and_then(|app| app.bot);
                if let Some(bot) = bot.filter(|bot| bot.id == user) {
                    due.push(ProfileDue { account: id, user, from: ReadFrom::Bot(bot.name) });
                    continue;
                }
            } else if renewing.iter().any(|renewing| renewing.account == account.id) {
                continue;
            } else if let Some(token) = self.token_to_read_with(files, &account, file, now_ms) {
                due.push(ProfileDue { account: id, user, from: ReadFrom::Github(token) });
                continue;
            }
            if self.nobody.lock().may_ask(now_ms) {
                due.push(ProfileDue { account: id, user, from: ReadFrom::Nobody });
            }
        }
        Ok(due)
    }

    /// The token to ask GitHub who `account` is with, under the lock `files` holds: its login's, held in `file`, while it
    /// works at `now_ms` by its record, and GitHub hasn't refused it since it was connected. `None` for one whose file
    /// holds no token, one in a form only a newer build reads, or one that can't be read, which blocks as it is while
    /// it's the default's, as `token_of` says.
    fn token_to_read_with(
        &self,
        files: &Locked<'_, GithubLogin>,
        account: &GithubAccount,
        file: Option<LoginFile>,
        now_ms: i64,
    ) -> Option<GithubToken> {
        let held = account.held_login.filter(|held| held.works_at(now_ms))?;
        let refused = self.refused.lock().get(&account.id).copied();
        if refused.is_some_and(|refused_at_ms| refused_at_ms >= held.connected_at_ms) {
            return None;
        }
        let Ok(Stored::Secret(login)) = files.read(&file?) else { return None };
        match login {
            GithubLogin::Gh { user } => self.gh_token(user, now_ms).ok(),
            login @ (GithubLogin::Token(_)
            | GithubLogin::SignIn(_)
            | GithubLogin::Installation(_)
            | GithubLogin::App(_)) => login.token().cloned(),
        }
    }

    /// Whether who `account` is on GitHub is due to be read: what GitHub calls its user isn't known, or, for any but an
    /// app's installation, what its profile says, or the last read of either was at `read_by` or before.
    fn read_due(&self, account: &GithubAccount, read_by: i64) -> bool {
        let username = self.observations.read_at::<Username>(&account.id);
        let read = if account.installation.is_some() {
            username
        } else {
            username
                .zip(self.observations.read_at::<GithubProfile>(&account.id))
                .map(|(name, profile)| name.min(profile))
        };
        read.is_none_or(|at_ms| at_ms <= read_by)
    }

    /// What asking GitHub who `due`'s account is came to, as GitHub was asked at `asked_at_ms`: what the hub heard, to
    /// keep, while the account is still there as the same user. GitHub naming another user than the account's is let go
    /// of. GitHub refusing the token is kept, so the next check asks with no login, and anything else, such as no
    /// answer, is asked again at the next check. What the hub kept before stays, as it was.
    pub(crate) fn profile_read(
        &self,
        due: &ProfileDue,
        read: Result<TokenOwner, ApiError>,
        asked_at_ms: i64,
    ) -> Result<Option<Heard>, ApiError> {
        let same = self.store.github_account(&due.account)?.is_some_and(|account| account.user.id == due.user);
        if !same {
            return Ok(None);
        }
        match read {
            Ok(owner) if owner.user.id == due.user => Ok(Some(Heard {
                account: due.account.clone(),
                username: owner.user.name,
                profile: Some(owner.profile),
                at_ms: asked_at_ms,
            })),
            Ok(owner) => {
                warn!(account = %due.account, "GitHub said its token is another user's, {}", owner.user.name);
                Ok(None)
            }
            Err(e) if e.code == ErrorCode::InvalidRequest => {
                warn!(account = %due.account, "GitHub didn't take its token, asked who it is: {}", e.message);
                self.refused.lock().insert(due.account.clone(), asked_at_ms);
                Ok(None)
            }
            Err(e) => {
                warn!(account = %due.account, "couldn't ask GitHub who it is, so the next check will: {}", e.message);
                Ok(None)
            }
        }
    }

    /// Works out what workers are sent again at `now_ms`, once the hub has kept what it heard of who an account is,
    /// which the default's credential is made from.
    pub(crate) fn heard_of(&self, now_ms: i64) -> Result<(), ApiError> {
        self.changing(now_ms, |_| Ok(()))
    }
}
