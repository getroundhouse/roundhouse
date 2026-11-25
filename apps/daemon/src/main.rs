//! The daemon as it ships.

use std::process::ExitCode;

fn main() -> anyhow::Result<ExitCode> {
    daemon::run(daemon::Config::new(protocol::this_build!()))
}
