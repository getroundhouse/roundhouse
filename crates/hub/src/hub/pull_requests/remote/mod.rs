//! The pull requests agents ask for and waits watch, which the hub reads from GitHub itself, as whoever the default
//! says. Overlapping waits of a workspace share a read, and every group due as the same token is read with the rest.

use super::{Asked, Hub, ReadsHeld};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{PullRequestNumber, PullRequestSelection, PullRequestsSnapshot, WorkspaceId};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
mod groups;
pub(super) use groups::GroupKey;
use groups::{Group, groups};
#[cfg(test)]
mod tests;

mod for_agents;
mod reading_as;
mod waits_read;
pub(in crate::hub) use reading_as::ReadsAs;
mod watching;
#[cfg(test)]
use watching::BACKOFF_MAX;
pub(super) use watching::RemoteSchedule;
#[cfg(test)]
use watching::due;
