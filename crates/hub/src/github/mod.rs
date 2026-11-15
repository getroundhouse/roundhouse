//! The hub's calls to GitHub's API, on Octocrab, each within a time limit, so a GitHub that doesn't answer fails only
//! the request that called it. Nothing calls it while holding a lock another request waits on.

#[cfg(test)]
use protocol::client::ErrorCode;
#[cfg(test)]
use protocol::model::GithubToken;

#[cfg(test)]
mod tests;

mod client;
#[cfg(test)]
use client::ASK_WITHIN;
pub(crate) use client::{Github, TokenOwner};
mod expiration;
#[cfg(test)]
use expiration::ends_at_ms;
#[cfg(test)]
mod stand_in;
#[cfg(test)]
pub(crate) use stand_in::{Answer, StandIn};
