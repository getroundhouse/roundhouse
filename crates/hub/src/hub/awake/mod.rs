//! Keeping this machine awake while agents work, so it doesn't go to sleep by itself partway through a turn. A program
//! does it for as long as it runs, and this is what starts it, watches it and stops it.
//!
//! The hub does it, and not a worker, since the hub knows what every session is doing and sends queued prompts itself.

#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use timing::Delays;
#[cfg(test)]
use tokio::task::JoinHandle;

#[cfg(test)]
mod tests;
mod timing;

mod inhibition;

pub(super) use inhibition::Awake;
use inhibition::Hold;
#[cfg(test)]
use inhibition::{Stimulant, hold_while};
