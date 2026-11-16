//! Renewing each sign-in with GitHub before its token ends, with what renews it, as the hub checks by the wall clock:
//! which are due, and keeping what GitHub gave in place of the tokens it revoked, or that it refused them. The hub asks
//! GitHub between the two, never under the logins' lock.

use std::time::Duration;

use protocol::client::ApiError;
use protocol::model::{GithubAccount, GithubAccountId};
use tracing::{error, info, warn};

use super::{GithubAccountChange, GithubLogin, GithubLogins, SignedIn};
use crate::error::internal;
use crate::github::{GithubRefreshToken, Renewal};
use crate::login_files::{Locked, LoginFile, Stored};
use crate::store::later_ms;

/// How much a sign-in's token has left when the hub renews it. GitHub's last 8 hours, so it's renewed about every 7,
/// and the check every minute tries sixty times before the token ends, so GitHub can fail to answer for an hour before
/// agents push as nobody. Renewing earlier would renew more often, and each renewal revokes the token a push under way
/// may be using. *Unconfirmed.*
pub(crate) const RENEWED_WITH_LEFT: Duration = Duration::from_hours(1);

/// A sign-in to renew, as a check found it: its account, and what renews its token, as its file held it then, which
/// tells, once GitHub has answered, whether the account holds that sign-in still.
#[derive(Debug)]
pub(crate) struct Due {
    pub(crate) account: GithubAccountId,
    pub(crate) refresh: GithubRefreshToken,
}

/// What a check of the logins found.
#[derive(Debug, Default)]
pub(crate) struct Checked {
    /// Each sign-in to renew now, at most one an account.
    pub(crate) due: Vec<Due>,
    /// The record of each account the check changed, as one it found nothing can renew.
    pub(crate) changed: Vec<GithubAccount>,
}

impl GithubLogins {
    /// Checks the logins at `now_ms` by the wall clock, as the hub does as it starts and every minute after. Each
    /// sign-in whose token has less than [`RENEWED_WITH_LEFT`] left, or has ended, is due, and one that nothing can
    /// renew, since GitHub gave nothing that renews it, or that has ended, needs signing in again, so it blocks once
    /// it's the default. Then what workers are sent is worked out again, since the default's token may have ended
    /// since, and the rest of those due are answered with, to renew. One that needs signing in again already isn't
    /// due, and nor is one whose file holds what this build can't renew, which blocks as it is.
    pub(crate) fn check(&self, now_ms: i64) -> Result<Checked, ApiError> {
        self.changing(now_ms, |files| {
            let renewed_before = later_ms(now_ms, RENEWED_WITH_LEFT);
            let mut checked = Checked::default();
            for (account, file) in self.store.held_github_sign_ins()? {
                let due = account.held_login.is_some_and(|held| {
                    !held.needs_sign_in && held.ends_at_ms.is_some_and(|ends_at_ms| ends_at_ms < renewed_before)
                });
                if !due {
                    continue;
                }
                // Another kind's, as no sign-in's file holds, one in a form only a newer build reads, none, or one that
                // can't be read, each of which blocks as it is while it's the default's, as `token_of` says.
                let Ok(Stored::Secret(GithubLogin::SignIn(signed_in))) = files.read(&file) else { continue };
                match signed_in.renews_with(now_ms) {
                    Some(refresh) => checked.due.push(Due { account: account.id, refresh: refresh.clone() }),
                    None => checked.changed.extend(self.store.github_login_needs_sign_in(&account.id)?),
                }
            }
            Ok(checked)
        })
    }

    /// Keeps what renewing `due` came to, as GitHub answered at `renewed_at_ms`, and answers with the account's record
    /// when that changed it: what GitHub gave in the login's file, in place, and then when it ends on the account's
    /// row, or, when GitHub refused what renews it, the account needing signing in again. Tokens GitHub gave that can't
    /// be kept need signing in again too, since GitHub revoked those the file holds as it gave them. What GitHub gave
    /// or refused for an account that has been disconnected, signed in again or renewed meanwhile is let go of, since
    /// its login isn't the one GitHub was asked about. Then what workers are sent is worked out again, as every change
    /// to a login has it, so a renewed default reaches every worker.
    pub(crate) fn renewed(
        &self,
        due: &Due,
        renewal: Renewal,
        renewed_at_ms: i64,
    ) -> Result<Option<GithubAccountChange>, ApiError> {
        self.changing(renewed_at_ms, |files| {
            let Some(file) = self.holding_still(files, due)? else {
                info!(account = %due.account, "let go of what renewing a sign-in came to, as it holds another now");
                return Ok(None);
            };
            let account = match renewal {
                Renewal::Renewed(granted) => {
                    self.keep_renewed(files, due, &file, SignedIn::given(granted, renewed_at_ms))?
                }
                Renewal::Refused(said) => {
                    warn!(account = %due.account, "GitHub refused what renews a sign-in's token: {said}");
                    self.store.github_login_needs_sign_in(&due.account)?
                }
            };
            Ok(account.map(|account| GithubAccountChange { account, changed: true, settings: None }))
        })
    }

    /// Keeps `renewed` in `file`, in place of what it held, and then when its token ends on `due`'s account's row, or,
    /// when it can't be kept, has the account need signing in again.
    fn keep_renewed(
        &self,
        files: &Locked<'_, GithubLogin>,
        due: &Due,
        file: &LoginFile,
        renewed: SignedIn,
    ) -> Result<Option<GithubAccount>, ApiError> {
        let ends_at_ms = renewed.token_ends_at_ms;
        if let Err(e) = files.replace(file, &GithubLogin::SignIn(renewed)) {
            error!(account = %due.account, "couldn't keep a sign-in's renewed tokens, so it needs signing in: {e}");
            return Ok(self.store.github_login_needs_sign_in(&due.account)?);
        }
        Ok(self.store.renew_github_sign_in(&due.account, ends_at_ms)?)
    }

    /// The file of `due`'s account's login, while it holds the sign-in `due` renews: one whose file holds what renews
    /// its token that GitHub was asked with.
    fn holding_still(&self, files: &Locked<'_, GithubLogin>, due: &Due) -> Result<Option<LoginFile>, ApiError> {
        let Some(file) = self.store.github_login_file(&due.account)? else { return Ok(None) };
        match files.read(&file) {
            Ok(Stored::Secret(GithubLogin::SignIn(held))) if held.refresh.as_ref() == Some(&due.refresh) => {
                Ok(Some(file))
            }
            Ok(Stored::Secret(_) | Stored::Newer | Stored::Nothing) => Ok(None),
            Err(e) => Err(internal(format!("couldn't read the login of the GitHub account {}: {e}", due.account))),
        }
    }
}
