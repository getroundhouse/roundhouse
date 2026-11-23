//! The hub's state, and what the rest of it shares: its workers, live sessions and feed.

pub(crate) use carrying_on::LeftAtWork;
#[cfg(test)]
pub(crate) use feed::Notices;
pub(crate) use feed::{Fed, Moves};
pub(crate) use followers::{Followed, Following};
pub(crate) use terminals::{Queued, Watcher};
pub use updates::UpdateFeed;

mod accounts;
mod agents;
mod attachments;
mod awake;
mod branches;
mod builds;
mod bundles;
mod carrying_on;
mod changes_watch;
mod children;
mod comments;
mod feed;
mod followers;
mod github_accounts;
mod github_apps;
mod github_observed;
mod github_reach;
pub(crate) mod github_reads;
mod github_sign_ins;
mod hub_settings;
mod instances;
mod logins;
mod new_workspace;
mod new_workspace_draft;
mod observations;
mod proposals;
mod pull_requests;
mod queues;
mod ready;
mod repo_source;
mod repo_urls;
mod repos;
mod sessions;
mod setup;
#[cfg(test)]
mod stand_in_workers;
mod stopping;
mod terminals;
#[cfg(test)]
pub(crate) mod tests;
mod titles;
mod tools;
mod updates;
mod waits;
mod workers;
mod workspace_lines;
mod workspaces;
mod worktree;

use crate::store::now_ms;

mod coordinator;
pub(crate) use coordinator::Hub;
use coordinator::{KEEPS_WORKERS, new_id, off_thread};
pub(crate) use dirs::HubDirs;
#[cfg(test)]
pub(crate) use observations::{WorkspaceChanges, WorkspaceLines};

mod devices;
mod dirs;
