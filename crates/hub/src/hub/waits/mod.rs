//! Agents' waits on their workspace's pull request: starting one from where the pull request stands, reading one, which
//! never reads GitHub, and ending them as each read of the pull request says. The hub keeps them, so they last across
//! its restarts, and the agent's MCP task is only a copy of one.

#[cfg(test)]
use super::{Hub, now_ms};
#[cfg(test)]
use crate::worker_link::WorkerLink;
#[cfg(test)]
use protocol::client::{ApiError, ErrorCode, HubNotice};
#[cfg(test)]
use protocol::model::{PullRequestFound, Wait, WaitEnd, WaitEnded, WaitId, WaitUntil};
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use std::time::Duration;

mod rules;
#[cfg(test)]
mod tests;

mod wait_calls;
