//! The calls about the GitHub accounts a hub holds, the default of which agents push and open pull requests as on every
//! machine: connecting a token, disconnecting one, choosing the default, and signing in with GitHub. A hub older than
//! these refuses them, as `is_unsupported` says, and its settings say nothing of GitHub.

use protocol::client::github_accounts;
use protocol::model::{GithubAccount, GithubAccountId, GithubSignIn, GithubSignInId, GithubToken, HubSettings};

use super::{Client, Result};

impl Client {
    /// Gives the hub `token` to hold as a GitHub account's login, as a new account, or in place of the login of
    /// `account`, as renewing it does, and answers with the account's record. See
    /// [`github_accounts::GithubAccountConnect`].
    pub fn github_account_connect(
        &self,
        token: GithubToken,
        account: Option<GithubAccountId>,
    ) -> Result<GithubAccount> {
        self.call(github_accounts::GithubAccountConnect { token, id: account })
    }

    /// Has the hub forget the login it holds for the GitHub account `id`, and answers with its record, which stays. See
    /// [`github_accounts::GithubAccountDisconnect`].
    pub fn github_account_disconnect(&self, id: &GithubAccountId) -> Result<GithubAccount> {
        self.call(github_accounts::GithubAccountDisconnect { id: id.clone() })
    }

    /// Makes `account` the default GitHub account, or each machine's own login with `None`, and answers with the hub's
    /// settings. See [`github_accounts::GithubAccountSetDefault`].
    pub fn github_account_set_default(&self, account: Option<GithubAccountId>) -> Result<HubSettings> {
        self.call(github_accounts::GithubAccountSetDefault { account })
    }

    /// Starts the sign-in with GitHub `id`, a new one from [`new_github_sign_in_id`], and answers with its record, with
    /// the code to type and where. See [`github_accounts::GithubSignInStart`].
    pub fn github_sign_in_start(&self, id: &GithubSignInId) -> Result<GithubSignIn> {
        self.call(github_accounts::GithubSignInStart { id: id.clone() })
    }

    /// Cancels the sign-in with GitHub `id`, if it's waiting, and answers with its record. See
    /// [`github_accounts::GithubSignInCancel`].
    pub fn github_sign_in_cancel(&self, id: &GithubSignInId) -> Result<GithubSignIn> {
        self.call(github_accounts::GithubSignInCancel { id: id.clone() })
    }
}

/// A new ID for a sign-in with GitHub, chosen by the client, so sending its start again is the same sign-in.
pub fn new_github_sign_in_id() -> GithubSignInId {
    GithubSignInId::from(uuid::Uuid::new_v4().to_string())
}
