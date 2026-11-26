//! A hub owns state and serves the platform API. It doesn't do the work itself: workers do, and
//! providers say where they run.

#![warn(clippy::wildcard_enum_match_arm)]

mod attachment_files;
mod cache_file;
mod calls;
mod child_process;
mod error;
mod frames;
mod gh;
mod github_cache;
mod github_logins;
mod hub;
mod jobs;
mod lasting;
mod listener;
mod login_files;
mod logins;
mod machine_environment;
mod names;
mod pasted_token;
mod provider_link;
mod provider_set;
mod run;
mod server;
mod session;
mod spans;
mod ssh;
pub mod store;
mod terminate;
mod timers;
mod tool_servers;
mod versions;
mod worker_link;

pub use crate::error::HubError;
pub use crate::hub::UpdateFeed;
pub use crate::run::{Ended, HubConfig, run};
pub use crate::timers::Timers;
pub use github::GithubConfig;
#[cfg(any(test, feature = "testing"))]
pub use github::{
    Answer as GithubAnswer, Gate as GithubGate, PullRequestState as GithubPullRequestState,
    PullRequests as GithubPullRequests, Script as GithubScript, StandIn as GithubStandIn,
};
#[cfg(any(test, feature = "testing"))]
pub use tool_sign_in::{
    Asks as ToolServerAsks, AuthorizationPage as ToolServerPage, Person as ToolServerPerson,
    Refreshing as ToolServerRefreshing, Script as ToolServerScript, StandIn as ToolServerStandIn,
    Wants as ToolServerWants,
};

/// The names the hub picks a new workspace's from, so a test can have git hold every one.
#[cfg(any(test, feature = "testing"))]
#[must_use]
pub fn workspace_names() -> &'static [&'static str] {
    names::NAMES
}

mod remote;
