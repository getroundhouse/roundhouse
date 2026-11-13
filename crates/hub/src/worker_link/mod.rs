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
use protocol::worker::{AskAnswer, HelloReply, WorkerCall, WorkerFrame, WorkerResponse};
#[cfg(test)]
use providers::Provider;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(test)]
use std::sync::{Arc, OnceLock};
#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use tokio::sync::{Mutex as AsyncMutex, watch};

mod acks;
mod reaching;
mod reading;

pub(crate) use reaching::Opening;

#[cfg(test)]
mod stand_in;
#[cfg(test)]
#[cfg(unix)]
mod tests;

mod connection;
#[cfg(test)]
use connection::{HELLO_TIMEOUT, NEXT_LINK_ID};
pub(crate) use connection::{STOP_GRACE, WorkerLink};
