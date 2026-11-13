//! The hub's state, and what the rest of it shares: its workers, live sessions and feed.

pub(crate) use carrying_on::LeftAtWork;
pub(crate) use workspaces::workspace_env;

mod accounts;
mod agents;
mod attachments;
mod awake;
mod branches;
mod builds;
mod carrying_on;
mod children;
mod comments;
mod feed;
mod hub_settings;
mod instances;
mod logins;
mod new_workspace;
mod new_workspace_draft;
mod pull_requests;
mod queues;
mod ready;
mod repos;
mod sessions;
mod setup;
mod stopping;
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
pub(super) use coordinator::SHUTDOWN_GRACE;
pub(crate) use coordinator::{BEFORE_SERVING, Hub, HubDirs};
use coordinator::{KEEPS_WORKERS, Wanted, new_id, off_thread};
