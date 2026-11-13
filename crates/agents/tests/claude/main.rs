//! The Claude adapter, against its captured output and against scripts standing in for the CLI.

#![cfg(test)]

mod background;
#[cfg(unix)]
mod conformance;
mod contract;
mod controls;
mod process;
mod results;
