//! Explicit remote PR reads and background waits, with the same dependency expansion and result vocabulary.

use super::mcp::Called;
use crate::asks::Asks;
use protocol::model::{
    CheckState, PullRequestSelection, PullRequestWatch, PullRequestsSnapshot, SessionId, Wait, WaitEnd, WaitUntil,
};
use protocol::worker::{ReadPullRequests, StartPullRequestWait};
use serde_json::{Map, Value, json};
#[cfg(test)]
mod tests;

mod calls;
pub(super) use calls::READ;
pub(super) use calls::WAIT;
#[cfg(test)]
use calls::arguments;
pub(super) use calls::definition;
pub(super) use calls::ending;
pub(super) use calls::progress;
pub(super) use calls::read;
#[cfg(test)]
use calls::read_arguments;
pub(super) use calls::result;
pub(super) use calls::start;
