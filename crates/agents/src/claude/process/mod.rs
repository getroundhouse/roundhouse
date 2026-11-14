//! Runs one `claude` process and supervises it across its whole life.
//!
//! The flags, the input format and every behavior relied on here were checked against the real CLI
//! (2.1.319). `docs/claude-cli.md` has the results.

#[cfg(test)]
use crate::AgentError;
#[cfg(test)]
use protocol::worker::AgentSpec;
#[cfg(test)]
use std::path::PathBuf;

mod asking;
mod compact;
mod context;
mod launch;
mod output;
mod plan;
mod prompt;
mod run_files;
mod supervise;
#[cfg(test)]
mod tests;
mod tools;
mod usage;

use launch::launch;
pub use launch::{SCRUBBED_ENV, build_args};
pub(super) use launch::{command, record, resolve_binary, start};
pub(super) use prompt::{image_block, user_line_saying};
use supervise::supervise;

mod running;

#[cfg(test)]
use running::refused;
use running::{Shared, ask, control, drain_grace, write_line, write_to};
pub use running::{spawn, spawn_at};

use crate::tap::{self, Tap};
#[cfg(feature = "testing")]
pub use running::spawn_tapped;
