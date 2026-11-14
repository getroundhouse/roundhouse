//! Shared reads for explicit waits. A workspace is the credential boundary: another worker may use another login.

use super::{Asked, Hub, ReadsHeld};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GitHubRepository, PullRequestNumber, PullRequestSelection, PullRequestsSnapshot, WorkspaceId};
use protocol::worker::ReadRemotePullRequests;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
mod groups;
use crate::worker_link::WorkerLink;
pub(super) use groups::GroupKey;
use groups::{Group, groups};
#[cfg(test)]
mod tests;

mod watching;
#[cfg(test)]
use watching::BACKOFF_MAX;
pub(super) use watching::RemoteSchedule;
#[cfg(test)]
use watching::oldest_due;
