//! The hub's state, and what the rest of it shares: its workers, live sessions and feed.

pub(crate) use carrying_on::LeftAtWork;
#[cfg(test)]
pub(crate) use terminals::QUEUED_UPDATES;
pub(crate) use terminals::{Following, Numbered, Queued};

mod accounts;
mod agents;
mod attachments;
mod awake;
mod branches;
mod builds;
mod bundles;
mod carrying_on;
mod children;
mod comments;
mod feed;
mod found_github_logins;
mod github_accounts;
mod github_apps;
mod github_reach;
mod github_sign_ins;
mod hub_settings;
mod instances;
mod logins;
mod new_workspace;
mod new_workspace_draft;
mod proposals;
mod pull_requests;
mod queues;
mod ready;
mod references;
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
mod waits;
mod workers;
mod workspaces;
mod worktree;

use crate::store::now_ms;

mod coordinator;
pub(crate) use coordinator::{Hub, HubDirs};
use coordinator::{KEEPS_WORKERS, Wanted, new_id, off_thread};

mod devices;
