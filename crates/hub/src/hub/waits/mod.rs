//! Agents' waits on explicit pull requests: starting one from a read of them, reading one, which never reads GitHub,
//! and ending them as each read of their pull requests says. The hub keeps them, so they last across its restarts, and
//! the agent's MCP task is only a copy of one.

use super::{Hub, new_id, now_ms};
#[cfg(test)]
use crate::worker_link::WorkerLink;
#[cfg(test)]
use protocol::client::{ApiError, ErrorCode, HubNotice};
#[cfg(test)]
use protocol::model::{Wait, WaitEnd, WaitEnded, WaitId};
#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
mod tests;

mod wait_calls;

mod remote;
mod remote_rules;
use wait_calls::START_WITHIN;
