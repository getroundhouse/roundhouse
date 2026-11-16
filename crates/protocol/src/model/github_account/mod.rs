//! The GitHub accounts the hub holds a login for, which agents push and open pull requests as, and the login found on
//! each provider's machines, which they push as while the default is the machine's own.

#[cfg(test)]
use super::{GithubAccountId, GithubSignInId, GithubUserCode, GithubUsername, ProviderName, Rev};

#[cfg(test)]
mod tests;

mod found;
pub use found::{FoundGithubLogin, GhLogin, GhLoginWorks};
mod record;
pub use record::{GithubAccount, GithubLoginKind, GithubToken, GithubUser, GithubUserId, HeldGithubLogin};
mod sign_in;
pub use sign_in::{GithubSignIn, GithubSignInStatus};
