//! What the worker asks its hub, for its agents' calls to its tools, and the answers it waits for.

#[cfg(test)]
use protocol::client::{ApiError, ErrorCode};
#[cfg(test)]
use protocol::worker::{AskAnswer, AskId, AskOutcome, AskReply, WorkerFrame};
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use tokio::sync::mpsc;

#[cfg(test)]
mod tests;

mod pending;

pub(crate) use pending::Asks;
