//! The pull requests agents ask for and waits watch, which the hub reads from GitHub itself, as whoever the default
//! says. Overlapping waits of a workspace share an ask, and are read with the workspace's own pull request.

use super::{Asked, Hub, ReadsHeld};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{PullRequestSelection, PullRequestsSnapshot, WorkspaceId};
use std::sync::Arc;
use tokio::time::Instant;
mod groups;
pub(super) use groups::{Waited, waited};
#[cfg(test)]
mod tests;

mod for_agents;
mod reading_as;
pub(in crate::hub) use reading_as::{NoToken, ReadsAs};
