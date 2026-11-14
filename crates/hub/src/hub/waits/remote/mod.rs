//! Starts and advances explicit-target waits through the same persisted tasks and wake/cancel paths as older waits.

use super::{Hub, START_WITHIN, new_id, now_ms, remote_rules};
use crate::store::Seen;
use crate::worker_link::WorkerLink;
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

mod wait_calls;
