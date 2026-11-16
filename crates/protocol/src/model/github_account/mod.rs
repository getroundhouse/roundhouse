//! The GitHub accounts the hub holds a login for, which agents push and open pull requests as.

#[cfg(test)]
use super::{GithubAccountId, GithubUsername, Rev};

#[cfg(test)]
mod tests;

mod record;
pub use record::{GithubAccount, GithubLoginKind, GithubToken, GithubUser, GithubUserId, HeldGithubLogin};
