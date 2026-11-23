use protocol::client::ApiError;
use protocol::model::{GithubAccountId, GithubSignIn, GithubSignInId, GithubSignInStatus};

use super::{GithubAccountChange, GithubLogin, GithubLogins, SignedIn};
use crate::store::{GithubApproved, GithubApproving, GithubConnected, StoreError};
use github::TokenOwner;

/// What GitHub gave a sign-in the person approved, to keep.
pub(crate) struct Approved {
    pub(crate) sign_in: GithubSignInId,
    pub(crate) login: SignedIn,
    /// The ID of the account it makes, if the hub holds no sign-in as the same GitHub user.
    pub(crate) new_id: GithubAccountId,
    /// When GitHub gave it, by the hub's clock.
    pub(crate) signed_in_at_ms: i64,
}

impl Approved {
    pub(crate) fn new(sign_in: GithubSignInId, login: SignedIn, signed_in_at_ms: i64) -> Self {
        let new_id = GithubAccountId::from(uuid::Uuid::new_v4().to_string());
        Self { sign_in, login, new_id, signed_in_at_ms }
    }
}

/// How a sign-in ends, when it isn't approved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SignInEnd {
    Refused,
    Ended,
    Cancelled,
    /// With why, as a person reads it.
    Failed(String),
}

impl SignInEnd {
    fn status(self) -> GithubSignInStatus {
        match self {
            SignInEnd::Refused => GithubSignInStatus::Refused,
            SignInEnd::Ended => GithubSignInStatus::Ended,
            SignInEnd::Cancelled => GithubSignInStatus::Cancelled,
            SignInEnd::Failed(message) => GithubSignInStatus::Failed { message },
        }
    }
}

/// A sign-in's record after it was approved or ended, whether that changed it, and the account approving it made or
/// renewed, so the hub announces only what changed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SignInChange {
    pub(crate) sign_in: GithubSignIn,
    pub(crate) changed: bool,
    pub(crate) account: Option<GithubAccountChange>,
}

impl SignInChange {
    fn unchanged(sign_in: GithubSignIn) -> Self {
        Self { sign_in, changed: false, account: None }
    }
}

impl GithubLogins {
    /// Ends the sign-in `id` as `end` says, unless it has ended already, as when GitHub approved it as a client
    /// cancelled it. Under the lock approving takes, so one that's approved is never then ended, and one ended never
    /// approved. It changes no login, so what workers are sent stays as it is. Refused with `NotFound` when the hub has
    /// no record of it.
    pub(crate) fn end_sign_in(&self, id: &GithubSignInId, end: SignInEnd) -> Result<SignInChange, ApiError> {
        let _files = self.files.lock();
        let sign_in = self.sign_in(id)?;
        if !sign_in.status.is_waiting() {
            return Ok(SignInChange::unchanged(sign_in));
        }
        let sign_in = self.store.set_github_sign_in(id, &end.status())?;
        Ok(SignInChange { sign_in, changed: true, account: None })
    }

    /// Keeps what GitHub gave `approved`'s sign-in, which GitHub said is `owner`'s, as the login of the account the hub
    /// holds a sign-in for as that GitHub user, in place of the one it held, or else of a new account, which becomes
    /// the default if it's the first the hub has a record of. Unless the sign-in has ended meanwhile, as when a client
    /// cancelled it, which keeps nothing. The login is kept, and then the account's row and the sign-in's written in
    /// one transaction, as [`Locked::connect`](crate::login_files::Locked::connect) says, and then what workers are
    /// sent worked out again, as every change to a login has it, so a default it renews or makes reaches every worker.
    pub(crate) fn approve_sign_in(&self, approved: &Approved, owner: &TokenOwner) -> Result<SignInChange, ApiError> {
        let change = self.changing(approved.signed_in_at_ms, |files| {
            let sign_in = self.sign_in(&approved.sign_in)?;
            if !sign_in.status.is_waiting() {
                return Ok(SignInChange::unchanged(sign_in));
            }
            let login = GithubLogin::SignIn(approved.login.clone());
            let (sign_in, account) = files.connect(&login, |file| {
                let approving = GithubApproving {
                    sign_in: &approved.sign_in,
                    user: &owner.user,
                    file,
                    new_account: &approved.new_id,
                    signed_in_at_ms: approved.signed_in_at_ms,
                    ends_at_ms: approved.login.token_ends_at_ms,
                };
                let GithubApproved { connected, sign_in } = self.store.approve_github_sign_in(&approving)?;
                let GithubConnected { account, replaced, settings } = connected;
                Ok(((sign_in, GithubAccountChange { account, changed: true, settings }), replaced))
            })?;
            Ok(SignInChange { sign_in, changed: true, account: Some(account) })
        })?;
        let account = change.account.map(|account| account.settled(&self.store)).transpose()?;
        Ok(SignInChange { account, ..change })
    }

    fn sign_in(&self, id: &GithubSignInId) -> Result<GithubSignIn, ApiError> {
        let found = self.store.github_sign_in(id)?;
        Ok(found.ok_or_else(|| StoreError::NotFound { kind: "sign-in with GitHub", id: id.to_string() })?)
    }
}
