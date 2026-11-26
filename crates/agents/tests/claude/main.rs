//! The Claude adapter, against its captured output and against scripts standing in for the CLI.

#![cfg(test)]

mod background;
#[cfg(unix)]
mod conformance;
mod contract;
mod controls;
mod covering;
mod process;
mod results;
mod run_files;
mod tool_servers;

mod login;
