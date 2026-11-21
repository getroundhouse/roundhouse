//! The hub's calls to GitHub: its API, its device flow, for signing in, what's asked as an app of your own, and its
//! git, for whether a push would be taken. They're on Octocrab, since GitHub has no Rust client of its own. Each is
//! within a time limit, so a GitHub that doesn't answer fails only the request, the sign-in or the check that called
//! it. Nothing calls it while holding a lock another request waits on. Only the hub calls it, and it knows nothing of
//! the hub: what it's given and what GitHub answers.

#![warn(clippy::wildcard_enum_match_arm)]

#[cfg(test)]
use protocol::client::ErrorCode;
#[cfg(test)]
use protocol::model::GithubToken;

#[cfg(test)]
mod tests;

mod apps;
pub use apps::{GithubAppKey, Installation, MadeApp, Minted, manifest};
mod client;
pub use client::{ASK_WITHIN, Github, TokenOwner};
mod config;
pub use config::GithubConfig;
mod device_flow;
pub use device_flow::{Approval, DeviceCode, GithubRefreshToken, Granted, Renewal};
mod expiration;
mod profile;
pub use profile::GithubProfile;
mod pull_requests;
pub use pull_requests::{
    BRANCHES_A_QUERY, Headed, HeadsAsk, HeadsRead, Leaving, PullRequestsAsk, PullRequestsRead, RateLimit, SummaryRead,
    Unread,
};
mod reach;
mod references;
#[cfg(test)]
use expiration::ends_at_ms;
pub use reach::PushAnswer;
pub use references::ReferencesRead;
#[cfg(any(test, feature = "testing"))]
mod stand_in;
#[cfg(any(test, feature = "testing"))]
pub use stand_in::{Answer, Gate, PullRequestState, PullRequests, References, Script, StandIn};
