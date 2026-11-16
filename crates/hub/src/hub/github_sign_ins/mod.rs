//! Signing in with GitHub, as a client asks: the hub asks GitHub for a code, records the sign-in, and waits for GitHub's
//! answer on a job of its own, which ends with the sign-in. What GitHub answers is `crate::github`'s to read, and what
//! approving keeps, and in what order, is `crate::github_logins`'s.

#[cfg(test)]
use protocol::client::ErrorCode;
#[cfg(test)]
use protocol::client::github_accounts::GithubSignInStart;

#[cfg(test)]
mod tests;

mod sign_in_calls;
mod waiting;
