//! The GitHub accounts the hub holds a login for, which agents push and open pull requests as, the apps of your own
//! whose installations some of them are, the login found on each provider's machines, which they push as while the
//! default is the machine's own, and whether the default can push to the repository on GitHub that each repository's
//! `origin` names, and what the hub sends every worker for the default.

#[cfg(test)]
use super::{
    GithubAccountId, GithubAppSlug, GithubSettings, GithubSignInId, GithubUserCode, GithubUsername, ProviderName, Rev,
};

#[cfg(test)]
mod tests;

mod app;
pub use app::{GithubApp, GithubInstallation};
mod found;
pub use found::{FoundGithubLogin, GhLogin, GhLoginWorks};
mod numbers;
pub use numbers::{GithubAppId, GithubInstallationId, GithubUserId};
mod reach;
pub use reach::{GithubReach, GithubRepositoryName, GithubRepositoryNameError, InstalledApp, Reach};
mod record;
pub use record::{GithubAccount, GithubLoginKind, GithubToken, GithubUser, HeldGithubLogin};
mod sent;
pub use sent::{SentCredential, UnusableLogin};
mod sign_in;
pub use sign_in::{GithubSignIn, GithubSignInStatus};
