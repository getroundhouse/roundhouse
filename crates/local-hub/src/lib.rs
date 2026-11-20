//! Starting a hub on this machine when there's none, which the desktop app and the CLI do so their user never has to:
//! which daemon it runs, and running it apart from the client. A phone never starts one, so this is apart from
//! `client-core`, which every client shares. A stamped build's hubs run from its own copy of the daemon, for the reason
//! "Updating the app" in `docs/architecture.md` gives, and any other build starts the daemon where it is. On Windows a
//! stamped build's hub runs from a second name for its copy that's the same for every build, as `fixed` says. Starting
//! one is also when the copies that nothing runs from anymore are removed.

#[cfg(any(windows, test))]
mod fixed;
mod old_copies;

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

#[cfg(test)]
use daemon::start_hub;
use daemon::temporary_prefix;
pub use daemon::{Daemon, connect_or_start};
