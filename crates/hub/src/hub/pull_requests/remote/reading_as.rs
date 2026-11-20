//! Whose token the hub reads a workspace's pull requests with: the default account's, the hub's own identity, wherever
//! the workspace runs, as `docs/decisions/github-on-provisioned-machines.md` says.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GithubAccountId, GithubRepositoryName, GithubToken, Workspace};

use super::Hub;
use crate::github_logins::DefaultCredential;
use crate::jobs::Limits;
use github::ASK_WITHIN;
use github::RateLimit;

/// The default account a read goes as, whose token's limit it counts against.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(in crate::hub) struct ReadsAs(pub(in crate::hub) GithubAccountId);

impl std::fmt::Display for ReadsAs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the default account, {}", self.0)
    }
}

impl Hub {
    /// Who pull requests are read as, as the default says now. With no default, or one that blocks, as once it was
    /// disconnected, a read fails, as one that may work later, once an account is connected or another is chosen, so
    /// a wait goes on waiting.
    pub(in crate::hub) fn reads_as(&self) -> Result<ReadsAs, ApiError> {
        match &*self.git_credential().borrow() {
            DefaultCredential::Account { account, .. } => Ok(ReadsAs(account.clone())),
            DefaultCredential::NoAccount { reason } | DefaultCredential::Blocked { reason, .. } => {
                Err(ApiError::new(ErrorCode::Worker, reason.clone()))
            }
        }
    }

    /// The token to read as `reads_as` with, while it's still the default. Fails when the default is another now, as a
    /// read that may work later, since the default can change back.
    pub(in crate::hub) fn token_of(&self, reads_as: &ReadsAs) -> Result<GithubToken, ApiError> {
        match &*self.git_credential().borrow() {
            DefaultCredential::Account { account, credential, .. } if *account == reads_as.0 => {
                Ok(credential.token.clone())
            }
            DefaultCredential::Account { .. }
            | DefaultCredential::NoAccount { .. }
            | DefaultCredential::Blocked { .. } => {
                Err(ApiError::new(ErrorCode::Worker, "the default GitHub account changed as it was read"))
            }
        }
    }

    /// Who pull requests are read as, with the token, as [`Hub::reads_as`] says.
    pub(in crate::hub) fn reading_as(&self) -> Result<(ReadsAs, GithubToken), ApiError> {
        let reads_as = self.reads_as()?;
        let token = self.token_of(&reads_as)?;
        Ok((reads_as, token))
    }

    /// Whether a read as `reads_as` that nobody waits on, such as of what a chat links to, can go now, leaving the part of
    /// GitHub's limit kept for agents and the person, as a read in turn does.
    pub(in crate::hub) fn may_read_unasked(&self, reads_as: &ReadsAs) -> Result<(), ApiError> {
        self.pull_requests.limits.allows(reads_as, super::Asked::InTurn, crate::store::now_ms()).map_err(|_| {
            ApiError::new(ErrorCode::Worker, "GitHub's limit for reading it is nearly used up until it resets")
        })
    }

    /// What's left of `reads_as`'s token's limit, as GitHub said as it answered a read.
    pub(in crate::hub) fn github_said(&self, reads_as: &ReadsAs, rate_limit: Option<RateLimit>) {
        self.pull_requests.limits.said(reads_as, rate_limit);
    }

    /// How long a read that makes `queries` has: the time the hub gives a call to GitHub for each. Nothing is left to
    /// do after it.
    pub(in crate::hub) fn read_limits(queries: u32) -> Limits {
        Limits { work: ASK_WITHIN.saturating_mul(queries), after: std::time::Duration::ZERO }
    }

    /// Which repository on GitHub the workspace's is, as [`Hub::github_repository_of`] says, for reading its pull
    /// requests, which one that isn't on github.com has none of.
    pub(in crate::hub) async fn on_github(&self, workspace: &Workspace) -> Result<GithubRepositoryName, ApiError> {
        self.github_repository_of(workspace).await?.ok_or_else(|| {
            let message = "the workspace's repository has no address on github.com the hub knows of, so the hub can't \
                           read its pull requests";
            ApiError::new(ErrorCode::Unavailable, message)
        })
    }
}
