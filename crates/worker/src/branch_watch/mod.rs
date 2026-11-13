//! Watching a worktree's branches, so the hub hears when the agent, or anyone, commits, switches branch or pushes,
//! rather than only when a turn ends. The OS says when git writes one of the files that hold them, so nothing runs
//! between changes, and git is run to read them only once it has finished writing, and every few minutes in case the
//! OS missed something.

#[cfg(test)]
use crate::git::branches;
#[cfg(test)]
use crate::serve::Outbox;
#[cfg(test)]
use notify::event::{AccessKind, AccessMode};
#[cfg(test)]
use notify::{Event, EventKind, Watcher};
#[cfg(test)]
use parking_lot::Mutex;
#[cfg(test)]
use protocol::worker::WorkerFrame;
#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use tokio::sync::{Notify, oneshot, watch};

#[cfg(test)]
pub(crate) mod tests;

mod watching;

#[cfg(test)]
use watching::{Wakes, mattered, start};
pub(crate) use watching::{Whose, watch};
