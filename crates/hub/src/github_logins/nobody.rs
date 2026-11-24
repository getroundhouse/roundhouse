//! Asking GitHub who an account is with no login, by its user's number, for an account whose own login can't be asked
//! with, as one whose token has ended: how often, and what an answer comes to. GitHub takes 60 such calls an hour from
//! one address, which anything else on the machine counts against too, so the hub keeps well under that, and stops for
//! an hour once GitHub says it's limiting them.

use std::collections::VecDeque;
use std::time::Duration;

use github::{NotRead, TokenOwner};
use protocol::client::ApiError;
use tracing::warn;

use super::GithubLogins;
use super::profiles::{Heard, ProfileDue};

/// How many times in an hour the hub asks GitHub who an account is with no login, a third of what GitHub takes, so a
/// hub with more accounts whose login can't be used than this names the rest in the hours after, and anything else on
/// the machine that asks GitHub with no login has the rest. *Unconfirmed.*
pub(crate) const NOBODY_ASKS_AN_HOUR: usize = 20;

/// How long the hub asks nothing with no login once GitHub has said it's limiting those calls, which GitHub counts by
/// the hour. *Unconfirmed.*
const LIMITED_FOR: Duration = Duration::from_hours(1);

/// When the hub has asked GitHub with no login, in memory alone, as [`GithubLogins::profiles_due`] spends it.
#[derive(Debug, Default)]
pub(crate) struct NobodyAsks {
    /// When each ask in the last hour was made, oldest first.
    asked_at_ms: VecDeque<i64>,
    /// Until when the hub asks nothing, since GitHub said it was limiting such calls.
    limited_until_ms: Option<i64>,
}

impl NobodyAsks {
    /// Whether the hub may ask GitHub with no login at `now_ms`, and keeps that it does if so: fewer than
    /// [`NOBODY_ASKS_AN_HOUR`] asks in the hour before, and none while GitHub is limiting them.
    pub(super) fn may_ask(&mut self, now_ms: i64) -> bool {
        if self.limited_until_ms.is_some_and(|until_ms| now_ms < until_ms) {
            return false;
        }
        let hour_ms = i64::try_from(Duration::from_hours(1).as_millis()).unwrap_or(i64::MAX);
        while self.asked_at_ms.front().is_some_and(|&at_ms| at_ms <= now_ms.saturating_sub(hour_ms)) {
            self.asked_at_ms.pop_front();
        }
        if self.asked_at_ms.len() >= NOBODY_ASKS_AN_HOUR {
            return false;
        }
        self.asked_at_ms.push_back(now_ms);
        true
    }
}

impl GithubLogins {
    /// What asking GitHub with no login who `due`'s account is came to, as asked at `asked_at_ms`: what the hub heard, to
    /// keep, while the account is still there as the same user, and its profile but for an app's installation, whose
    /// bot has none. Anything else leaves the account as it was, not known if it wasn't, and decides nothing: GitHub
    /// limiting such calls stops them for [`LIMITED_FOR`], and any other failure is asked again at the next check
    /// [`NobodyAsks`] lets ask.
    pub(crate) fn nobody_read(
        &self,
        due: &ProfileDue,
        read: Result<TokenOwner, NotRead>,
        asked_at_ms: i64,
    ) -> Result<Option<Heard>, ApiError> {
        let owner = match read {
            Ok(owner) => owner,
            Err(NotRead::Limited) => {
                warn!(account = %due.account, "GitHub is limiting calls with no login, so none is made for an hour");
                let limited_for = i64::try_from(LIMITED_FOR.as_millis()).unwrap_or(i64::MAX);
                self.nobody.lock().limited_until_ms = Some(asked_at_ms.saturating_add(limited_for));
                return Ok(None);
            }
            Err(NotRead::Failed(e)) => {
                warn!(account = %due.account, "couldn't ask GitHub who it is with no login: {}", e.message);
                return Ok(None);
            }
        };
        let Some(account) = self.store.github_account(&due.account)? else { return Ok(None) };
        if account.user.id != due.user || owner.user.id != due.user {
            return Ok(None);
        }
        let profile = account.installation.is_none().then_some(owner.profile);
        Ok(Some(Heard { account: due.account.clone(), username: owner.user.name, profile, at_ms: asked_at_ms }))
    }
}
