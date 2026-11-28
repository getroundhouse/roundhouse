//! The hub's end of one worker connection.

#[cfg(test)]
use crate::calls::Calls;
#[cfg(test)]
use crate::child_process::ChildProcess;
#[cfg(test)]
use crate::hub::Hub;
#[cfg(test)]
use crate::versions;
#[cfg(test)]
use acks::Acks;
#[cfg(test)]
use protocol::Build;
#[cfg(test)]
use protocol::client::{ApiError, ErrorCode};
#[cfg(test)]
use protocol::model::InstanceId;
#[cfg(test)]
use protocol::worker::{AskAnswer, HelloReply, WorkerFrame, WorkerResponse};
#[cfg(test)]
use providers::Provider;
#[cfg(test)]
use std::sync::atomic::AtomicBool;
#[cfg(test)]
use std::sync::{Arc, OnceLock};
#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use tokio::sync::{Mutex as AsyncMutex, watch};

mod acks;
mod at_once;
mod carrier;
mod following;
mod greeting;
mod reaching;
mod reading;

#[cfg(test)]
pub(crate) use at_once::ANSWERED_WITHIN;
pub(crate) use at_once::answered_at_once;
pub(crate) use reaching::Opening;
#[cfg(test)]
pub(crate) use reaching::dial_unless_gone;

#[cfg(test)]
mod stand_in;
#[cfg(test)]
pub(crate) use stand_in::Answer;
#[cfg(test)]
#[cfg(unix)]
mod tests;

mod connection;
mod link_id;
pub(crate) use connection::{STOP_GRACE, WorkerLink};
pub(crate) use link_id::WorkerLinkId;
