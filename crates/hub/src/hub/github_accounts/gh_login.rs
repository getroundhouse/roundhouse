//! The login `gh` has on the hub's own machine: who it is, as a client asks to offer it, connecting it as an account's
//! login, as only a person asks, and reading its token again at each check of the logins, for as long as an account
//! holds it. `gh` is run and GitHub asked between two turns at the logins' lock, never under it. What `gh` prints is
//! `crate::gh`'s to read, and what's kept of it `crate::github_logins`'s.

use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GhLogin, GhLoginWorks, GithubAccount, GithubToken, GithubUser, GithubUsername};
use tracing::warn;

use super::super::{Hub, off_thread};
use crate::gh::GhRead;
use crate::github_logins::{GhSaid, NoGhLogin};
use crate::store::now_ms;
use github::TokenOwner;

/// What reading `gh` came to: who it says it's logged in as, if anyone, and what that comes to as a login.
struct GhAsked {
    username: Option<GithubUsername>,
    said: Said,
}

/// What a login of `gh`'s comes to, as [`GhSaid`] says, with who GitHub said its token is, `owner`, when it was asked
/// rather than known from the last read.
enum Said {
    Works { user: GithubUser, token: GithubToken, owner: Option<TokenOwner> },
    NoLogin(NoGhLogin),
    Unread(String),
}

impl Said {
    fn said(self) -> GhSaid {
        match self {
            Said::Works { user, token, .. } => GhSaid::Works { user, token },
            Said::NoLogin(why) => GhSaid::NoLogin(why),
            Said::Unread(why) => GhSaid::Unread { why },
        }
    }
}

impl Hub {
    /// Who `gh` on the hub's machine is logged in as, and whether GitHub takes its token, as `HubGhLogin` says.
    pub(crate) async fn hub_gh_login(&self) -> Result<GhLogin, ApiError> {
        let GhAsked { username, said } = self.ask_gh(false).await?;
        let Some(username) = username else {
            let installed = !matches!(said, Said::NoLogin(NoGhLogin::NotInstalled));
            return Ok(if installed { GhLogin::LoggedOut } else { GhLogin::NotInstalled });
        };
        let works = match said {
            Said::Works { .. } => GhLoginWorks::Yes,
            Said::NoLogin(NoGhLogin::Refused) => GhLoginWorks::Refused,
            Said::NoLogin(_) | Said::Unread(_) => GhLoginWorks::CouldntTell,
        };
        Ok(GhLogin::LoggedIn { username, works })
    }

    /// Has the hub hold the login `gh` has on its machine, as `GithubAccountConnectGh` says, and tells every client
    /// what that changed of the account's record, and of the settings when it made the account the default.
    pub(crate) async fn github_account_connect_gh(&self) -> Result<GithubAccount, ApiError> {
        let (owner, token) = match self.ask_gh(true).await?.said {
            Said::Works { owner: Some(owner), token, .. } => (owner, token),
            // Never, since a fresh ask always asks GitHub.
            Said::Works { owner: None, .. } => {
                return Err(ApiError::new(ErrorCode::Unavailable, "the hub couldn't ask GitHub whose gh's token is"));
            }
            Said::NoLogin(why @ NoGhLogin::Refused) => {
                return Err(ApiError::new(ErrorCode::InvalidRequest, refused(why)));
            }
            Said::NoLogin(why) => return Err(ApiError::new(ErrorCode::Conflict, refused(why))),
            Said::Unread(why) => return Err(ApiError::new(ErrorCode::Unavailable, why)),
        };
        let logins = Arc::clone(&self.github_logins);
        let change = off_thread(move || logins.connect_gh(&owner, token, now_ms())).await?;
        Ok(self.announce_github_account(change))
    }

    /// Reads `gh` again, as a check of the logins does while an account holds its login, and keeps what that comes
    /// to, so what workers are sent follows `gh` logging out, switching, or renewing its token. A read that fails is
    /// said in the log, since it leaves the last token for a few minutes, and the rest is the default's block to say.
    pub(in super::super) async fn read_gh(&self) {
        let said = match self.ask_gh(false).await {
            Ok(asked) => asked.said.said(),
            Err(e) => GhSaid::Unread { why: e.message },
        };
        if let GhSaid::Unread { why } = &said {
            warn!("couldn't read gh's login on this machine, which an account uses: {why}");
        }
        let logins = Arc::clone(&self.github_logins);
        if let Err(e) = off_thread(move || logins.gh_read(said, now_ms())).await {
            warn!("couldn't keep what reading gh's login came to: {}", e.message);
        }
    }

    /// Reads `gh`, and asks GitHub whose its token is, unless it's the token the hub read last and `fresh` isn't asked
    /// for, since a token is only ever one user's. Refused only when the hub couldn't run `gh` at all.
    async fn ask_gh(&self, fresh: bool) -> Result<GhAsked, ApiError> {
        let read = self.gh.read().await.map_err(|e| ApiError::new(ErrorCode::Unavailable, e.to_string()))?;
        let (username, token, renews) = match read {
            GhRead::LoggedIn { username, token: Some(token), renews } => (username, token, renews),
            GhRead::LoggedIn { username, token: None, .. } => {
                return Ok(GhAsked { username: Some(username), said: Said::NoLogin(NoGhLogin::NoToken) });
            }
            GhRead::LoggedOut => {
                return Ok(GhAsked { username: None, said: Said::NoLogin(NoGhLogin::LoggedOut) });
            }
            GhRead::NotInstalled => {
                return Ok(GhAsked { username: None, said: Said::NoLogin(NoGhLogin::NotInstalled) });
            }
        };
        let known = self.github_logins.gh_known(now_ms());
        if !fresh && let Some((user, known)) = known.filter(|(_, known)| *known == token) {
            return Ok(GhAsked { username: Some(username), said: Said::Works { user, token: known, owner: None } });
        }
        Ok(GhAsked { username: Some(username), said: self.whose(token, renews).await })
    }

    /// Who GitHub says `token`, `gh`'s, is. One GitHub refuses that `gh` renews by itself is renewed, as a run of `gh`
    /// the person made would, and asked about again.
    async fn whose(&self, token: GithubToken, renews: bool) -> Said {
        let mut asked = self.github.token_owner(&token).await;
        let mut token = token;
        if renews
            && asked.as_ref().is_err_and(|e| e.code == ErrorCode::InvalidRequest)
            && let Some(renewed) = self.gh.renewed().await
        {
            asked = self.github.token_owner(&renewed).await;
            token = renewed;
        }
        match asked {
            Ok(owner) => Said::Works { user: owner.user.clone(), token, owner: Some(owner) },
            Err(e) if e.code == ErrorCode::InvalidRequest => Said::NoLogin(NoGhLogin::Refused),
            Err(e) => Said::Unread(e.message),
        }
    }
}

/// Why connecting `gh`'s login was refused, as `why` says, and what to do about it, then try again. *Unconfirmed:* the
/// words.
fn refused(why: NoGhLogin) -> String {
    let to_do = match why {
        NoGhLogin::NotInstalled => "Install gh, run gh auth login",
        NoGhLogin::LoggedOut | NoGhLogin::NoToken | NoGhLogin::Refused => "Run gh auth login",
    };
    // Each reason starts with gh's name, or GitHub's, so it starts a sentence as it is.
    format!("{why}. {to_do}, then try again.")
}
