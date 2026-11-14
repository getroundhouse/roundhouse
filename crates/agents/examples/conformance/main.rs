//! Runs native conformance where its process isolation is supported.
use std::process::ExitCode;

#[cfg(unix)]
mod runner;

#[cfg(unix)]
fn main() -> ExitCode {
    runner::run()
}

#[cfg(windows)]
fn main() -> ExitCode {
    eprintln!("The native conformance runner needs Unix process isolation.");
    ExitCode::FAILURE
}
