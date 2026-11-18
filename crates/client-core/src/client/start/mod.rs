//! Starting a hub when there's none, which a client does so its user never has to: which daemon it runs, and running
//! it apart from this client. A stamped build's hubs run from its own copy of the daemon, for the reason "Updating
//! the app" in `docs/architecture.md` gives, and any other build starts the daemon where it is. On Windows a stamped build's hub
//! runs from a second name for its copy that's the same for every build, as `fixed` says. Starting one is also
//! when the copies that nothing runs from anymore are removed.

#[cfg(any(windows, test))]
mod fixed;
mod old_copies;

#[cfg(test)]
use super::Client;
#[cfg(test)]
use protocol::paths::{DAEMON_NAME, Paths};
#[cfg(test)]
use protocol::{Build, StampedBuild};
#[cfg(test)]
use std::fs::{File, OpenOptions};
#[cfg(test)]
use std::io;
#[cfg(test)]
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::process::Command;
#[cfg(test)]
use std::time::Duration;

#[cfg(test)]
mod tests;

mod daemon;

pub use daemon::Daemon;
#[cfg(test)]
use daemon::start_hub;
use daemon::temporary_prefix;
