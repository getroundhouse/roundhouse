//! Connecting a GitHub account's token and disconnecting it, as a client asks, and choosing the default. Who a token is
//! is GitHub's to say, through `crate::github`, and what's kept, and in what order, is `crate::github_logins`'s.

#[cfg(test)]
use protocol::client::github_accounts::{GithubAccountConnect, GithubAccountSetDefault};
#[cfg(test)]
use protocol::client::{ErrorCode, HubNotice};

#[cfg(test)]
mod tests;

mod github_calls;
