//! Connecting a GitHub account's token and disconnecting it, as a client asks, choosing the default, what workers
//! give their processes for github.com from it, and renewing each sign-in's token before it ends. Who a token is, and
//! what renewing one gives, is GitHub's to say, through `crate::github`, and what's kept, in what order, and what that
//! has workers sent, is `crate::github_logins`'s.

#[cfg(test)]
use credential::{CHECKED_EVERY, check_by_the_wall_clock};
#[cfg(test)]
use protocol::client::github_accounts::{GithubAccountConnect, GithubAccountSetDefault};
#[cfg(test)]
use protocol::client::{ErrorCode, HubNotice};

#[cfg(test)]
mod tests;

mod credential;
mod github_calls;
mod renewals;
