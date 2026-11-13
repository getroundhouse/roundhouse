//! What a worker sends its hub, as the hub reads it: the reply to each call, what the worker asks, and what its agents
//! say, which goes to their sessions, or is held until the hub says which agent each session has.

#[cfg(test)]
use protocol::model::RunId;
#[cfg(test)]
use protocol::worker::WorkerFrame;
#[cfg(test)]
use std::collections::HashMap;

#[cfg(test)]
mod tests;

mod frames;

pub(super) use frames::{newest_runs, read_frames, route};
