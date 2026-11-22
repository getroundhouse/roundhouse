//! `iphone` builds the iPhone app, installs it on a phone connected to this Mac, and uploads it to `TestFlight`, then
//! waits for App Store Connect to process the upload, as `apps/mobile-ios/README.md` says. The `TestFlight` workflow
//! runs it: the job with no keys builds the app and this tool, and the job with the keys runs what that job built, so
//! no dependency compiles beside the keys.

mod args;
mod cargo;
mod changes;
mod connect;
mod git;
mod program;
mod stamp;
mod upload;
mod xcode;

use std::process::ExitCode;

use clap::Parser;

use crate::args::{Cli, Command};
use crate::cargo::Profile;
use crate::xcode::DeviceId;

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    let root = git::root()?;
    match cli.command {
        Command::Install { app, device, debug, no_install } => {
            let profile = if debug { Profile::Debug } else { Profile::Release };
            xcode::install(&root, &app.identify()?, &DeviceId::new(device), profile, !no_install)?;
        }
        Command::Compile { out } => cargo::compile_to(&root, &out)?,
        Command::Upload { app, key, binary, notes } => {
            upload::upload(&root, &app.identify()?, &key.read()?, binary.as_deref(), notes.as_deref())?;
        }
        Command::ChangedSince { commit } => {
            let changed = changes::changed_since(&root, &commit)?;
            return Ok(if changed { ExitCode::SUCCESS } else { ExitCode::from(1) });
        }
        Command::WhatToTest { since } => print!("{}", changes::what_to_test(&root, since.as_deref())?),
    }
    Ok(ExitCode::SUCCESS)
}
