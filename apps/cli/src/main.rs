//! Command-line client. It only talks to the hub, the same way the desktop app will.

mod agents;
mod args;
mod render;
mod run;
mod session;
mod workspace;

use clap::Parser;

fn main() {
    if let Err(e) = run::run(args::Args::parse()) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
