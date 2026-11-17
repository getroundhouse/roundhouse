//! The hub's calls to GitHub, on Octocrab: its API, its device flow, for signing in, and what's asked as an app of your
//! own. Each is within a time limit, so a GitHub that doesn't answer fails only the request, or the sign-in, that
//! called it. Nothing calls it while holding a lock another request waits on.

#[cfg(test)]
use protocol::client::ErrorCode;
#[cfg(test)]
use protocol::model::GithubToken;

#[cfg(test)]
mod tests;

mod apps;
pub(crate) use apps::{GithubAppKey, Installation, MadeApp, Minted, manifest};
mod client;
pub(crate) use client::{ASK_WITHIN, Github, TokenOwner};
mod config;
pub use config::GithubConfig;
mod device_flow;
pub(crate) use device_flow::{Approval, DeviceCode, GithubRefreshToken, Granted, Renewal};
mod expiration;
#[cfg(test)]
use expiration::ends_at_ms;
#[cfg(any(test, feature = "testing"))]
mod stand_in;
#[cfg(any(test, feature = "testing"))]
pub use stand_in::{Answer, Gate, Script, StandIn};
