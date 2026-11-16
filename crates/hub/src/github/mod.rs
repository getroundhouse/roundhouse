//! The hub's calls to GitHub, on Octocrab: its API, and its device flow, for signing in. Each is within a time limit,
//! so a GitHub that doesn't answer fails only the request, or the sign-in, that called it. Nothing calls it while
//! holding a lock another request waits on.

#[cfg(test)]
use protocol::client::ErrorCode;
#[cfg(test)]
use protocol::model::GithubToken;

#[cfg(test)]
mod tests;

mod client;
pub(crate) use client::{ASK_WITHIN, Github, TokenOwner};
mod config;
pub use config::GithubConfig;
mod device_flow;
pub(crate) use device_flow::{Approval, DeviceCode, GithubRefreshToken, Granted};
mod expiration;
#[cfg(test)]
use expiration::ends_at_ms;
#[cfg(any(test, feature = "testing"))]
mod stand_in;
#[cfg(any(test, feature = "testing"))]
pub use stand_in::{Answer, Script, StandIn};
