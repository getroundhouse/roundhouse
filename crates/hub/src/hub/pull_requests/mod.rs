//! Each workspace's pull request, which the hub keeps on the workspace's record, so every client shows every
//! workspace's as soon as it connects, and as it last was after a restart. It changes on GitHub while nobody's looking,
//! so the hub reads it again when a client asks, when an agent's turn ends, and on its own, within what GitHub can
//! spare.

#[cfg(test)]
use crate::store::now_ms;
#[cfg(test)]
use crate::worker_link::WorkerLink;
#[cfg(test)]
use protocol::client::{ApiError, ErrorCode};
#[cfg(test)]
use protocol::model::{BranchName, PullRequest, PullRequestFound, WorkspaceId};
#[cfg(test)]
use protocol::worker::PullRequestReply;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::Ordering;
#[cfg(test)]
use std::time::Duration;

mod budget;
mod schedule;
#[cfg(test)]
mod tests;

mod reading;

#[cfg(test)]
use reading::AGENT_SPACING;
use reading::Asked;
pub(super) use reading::{Reading, When};
