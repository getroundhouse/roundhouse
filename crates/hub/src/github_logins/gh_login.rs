//! The login `gh` has on the hub's machine, as a GitHub account's: what the hub last read of it, which it keeps in
//! memory alone, the token that works as the account while that read is fresh, and connecting it, which only a person
//! asks for. The hub reads `gh`, and asks GitHub whose its token is, between two turns at the logins' lock, never
//! under it.

use std::time::Duration;

use protocol::client::ApiError;
use protocol::model::{GithubAccountId, GithubLoginKind, GithubToken, GithubUser, GithubUserId};

use super::blocks::Unusable;
use super::{GithubAccountChange, GithubLogin, GithubLogins};
use crate::store::{GithubConnected, GithubConnecting};
use github::TokenOwner;

/// How long a token read from `gh` works as its account without being read again. The hub reads `gh` at every check
/// of its logins, once a minute, so this outlasts a few reads that fail, as while `gh` is slow to answer, and a token
/// `gh` no longer has, as once it has logged out, isn't used past it. *Unconfirmed.*
pub(crate) const GH_TOKEN_KEPT_FOR: Duration = Duration::from_mins(5);

/// What the hub last read of the `gh` login on its machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GhSaid {
    /// Logged in with `token`, which GitHub said is `user`'s.
    Works { user: GithubUser, token: GithubToken },
    /// `gh` has no login that works, as `why` says.
    NoLogin(NoGhLogin),
    /// `gh`, or GitHub, couldn't be asked, as `why` says.
    Unread { why: String },
}

/// Why `gh` on the hub's machine has no login that works.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoGhLogin {
    /// There's no `gh` on the machine's `PATH`.
    NotInstalled,
    /// `gh` is logged in to nothing for github.com.
    LoggedOut,
    /// `gh` names a login, and printed no token for it, as when its keyring has lost it.
    NoToken,
    /// GitHub refused the token `gh` printed, once `gh` had renewed it, if it renews it by itself.
    Refused,
}

/// What the hub's words call its own machine, which a person may be reading of on another device, or in a push's
/// refusal on another provider's machine, so never "this Mac" or "there". *Unconfirmed.*
pub(crate) const HUB_MACHINE: &str =
    if cfg!(target_os = "macos") { "the Mac Roundhouse runs on" } else { "the computer Roundhouse runs on" };

impl std::fmt::Display for NoGhLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoGhLogin::NotInstalled => write!(f, "gh isn't installed on {HUB_MACHINE}"),
            NoGhLogin::LoggedOut => write!(f, "gh on {HUB_MACHINE} is logged out"),
            NoGhLogin::NoToken => write!(f, "gh on {HUB_MACHINE} printed no token for its login"),
            NoGhLogin::Refused => write!(f, "GitHub refused the token of gh on {HUB_MACHINE}"),
        }
    }
}

/// [`GhSaid`], with when it was read, by the hub's clock.
#[derive(Clone, Debug)]
pub(super) struct GhKept {
    said: GhSaid,
    at_ms: i64,
}

impl GithubLogins {
    /// Whether the hub holds a login of `gh`'s as any account's, so its checks read `gh`.
    pub(crate) fn holds_gh(&self) -> Result<bool, ApiError> {
        Ok(!self.store.held_github_logins_of(GithubLoginKind::Gh)?.is_empty())
    }

    /// The token `gh` gave as the login of `user`'s account, unless it hasn't been read within [`GH_TOKEN_KEPT_FOR`]
    /// of `now_ms`, or is another user's now, or `gh` has none that works, as [`Unusable`] says.
    pub(super) fn gh_token(&self, user: GithubUserId, now_ms: i64) -> Result<GithubToken, Unusable> {
        let Some(kept) = self.gh.lock().clone() else { return Err(Unusable::GhUnread(None)) };
        match kept.said {
            GhSaid::Works { user: read, .. } if read.id != user => Err(Unusable::GhOtherUser(read.name)),
            GhSaid::Works { token, .. } if !stale(kept.at_ms, now_ms) => Ok(token),
            GhSaid::Works { .. } => Err(Unusable::GhUnread(None)),
            GhSaid::NoLogin(why) => Err(Unusable::GhLoggedOut(why)),
            GhSaid::Unread { why } => Err(Unusable::GhUnread(Some(why))),
        }
    }

    /// The token last read from `gh`, if it was read within [`GH_TOKEN_KEPT_FOR`] of `now_ms`, so a read that gives the
    /// same token needn't ask GitHub whose it is again, since a token is only ever one user's.
    pub(crate) fn gh_known(&self, now_ms: i64) -> Option<(GithubUser, GithubToken)> {
        let kept = self.gh.lock().clone()?;
        match kept.said {
            GhSaid::Works { user, token } if !stale(kept.at_ms, now_ms) => Some((user, token)),
            GhSaid::Works { .. } | GhSaid::NoLogin(_) | GhSaid::Unread { .. } => None,
        }
    }

    /// Keeps what reading `gh` came to at `read_at_ms`, then works out what workers are sent again, as every change
    /// to the logins has it, so `gh` logging out, or a token it renewed, reaches every worker. A read that failed leaves
    /// a token that still works until it's [`GH_TOKEN_KEPT_FOR`] old, so one slow `gh` doesn't block agents.
    pub(crate) fn gh_read(&self, said: GhSaid, read_at_ms: i64) -> Result<(), ApiError> {
        self.changing(read_at_ms, |_| {
            let mut kept = self.gh.lock();
            let keeps_last = matches!(said, GhSaid::Unread { .. })
                && kept
                    .as_ref()
                    .is_some_and(|kept| matches!(kept.said, GhSaid::Works { .. }) && !stale(kept.at_ms, read_at_ms));
            if !keeps_last {
                *kept = Some(GhKept { said, at_ms: read_at_ms });
            }
            Ok(())
        })
    }

    /// Has the hub hold the login `gh` has, whose token GitHub said is `owner`'s, as `GithubAccountConnectGh` says, at
    /// `now_ms`. An account that holds `gh`'s login as the same user already is that account again, and changes
    /// nothing. Otherwise a new account holds it, whose file says only whose it is, as [`GithubLogin::Gh`] says.
    pub(crate) fn connect_gh(
        &self,
        owner: &TokenOwner,
        token: GithubToken,
        now_ms: i64,
    ) -> Result<GithubAccountChange, ApiError> {
        self.changing(now_ms, |files| {
            *self.gh.lock() = Some(GhKept { said: GhSaid::Works { user: owner.user.clone(), token }, at_ms: now_ms });
            let login = GithubLogin::Gh { user: owner.user.id };
            if let Some(holding) = files.holding(self.store.held_github_logins()?, &login) {
                return Ok(GithubAccountChange::unchanged(self.record(&holding)?));
            }
            let id = GithubAccountId::from(uuid::Uuid::new_v4().to_string());
            let (account, settings) = files.connect(&login, |file| {
                let connecting = GithubConnecting {
                    account: &id,
                    user: &owner.user,
                    how: GithubLoginKind::Gh,
                    file,
                    connected_at_ms: now_ms,
                    ends_at_ms: None,
                    installation: None,
                };
                let GithubConnected { account, replaced, settings } = self.store.connect_github_login(&connecting)?;
                Ok(((account, settings), replaced))
            })?;
            Ok(GithubAccountChange { account, changed: true, settings })
        })?
        .settled(&self.store)
    }
}

/// Whether what was read at `at_ms` is too old at `now_ms` to use, as [`GH_TOKEN_KEPT_FOR`] says.
fn stale(at_ms: i64, now_ms: i64) -> bool {
    let kept_for = i64::try_from(GH_TOKEN_KEPT_FOR.as_millis()).unwrap_or(i64::MAX);
    now_ms.saturating_sub(at_ms) > kept_for
}
