//! Runs native conformance where its process isolation is supported.
use std::process::ExitCode;

#[cfg(unix)]
mod runner;

#[cfg(unix)]
fn main() -> ExitCode {
    // A trial on the real model runs the CLI in this process's environment, which may be a Claude session's, so this
    // forgets what the session set, as the daemon does.
    for name in agents::claude::PARENT_SESSION_ENV {
        // SAFETY: this runs on the main thread before `runner::run` starts its runtime, so before any other thread
        // exists, and nothing can be reading the environment.
        unsafe { std::env::remove_var(name) };
    }
    runner::run()
}

#[cfg(windows)]
fn main() -> ExitCode {
    eprintln!("The native conformance runner needs Unix process isolation.");
    ExitCode::FAILURE
}
