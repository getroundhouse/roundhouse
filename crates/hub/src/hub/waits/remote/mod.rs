//! Starts and advances waits on the pull requests an agent names, through the persisted tasks and the paths that wake
//! and cancel every wait.

use super::{Hub, START_WITHIN, new_id, now_ms, remote_rules};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    GitHubRepository, PullRequestNumber, PullRequestWatch, PullRequestsSnapshot, SessionId, Wait, WaitEnd, WaitEnded,
    WaitUntil, WorkspaceId,
};
use protocol::worker::{PullRequestWaitUntil, ReadPullRequests, StartPullRequestWait, WaitAnswer};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
#[cfg(test)]
mod tests;

mod reads;
mod wait_calls;
