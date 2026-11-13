//! Moving each workspace's worker onto this hub's build. A worker an earlier hub left running can be another build,
//! as after an update, and this hub carries on with it while anything in its workspace is at work, so an update never
//! cuts an agent short. Once nothing is, it stops that worker, and the next request for the workspace starts its own.

#[cfg(test)]
use super::Hub;
#[cfg(test)]
use crate::worker_link::WorkerLink;
#[cfg(test)]
use protocol::model::WorkspaceId;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use std::time::Duration;

#[cfg(test)]
mod tests;

mod build_records;
#[cfg(test)]
use build_records::LOOK_EVERY;
