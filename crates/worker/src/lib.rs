//! A worker does the work: it runs agents and git for one workspace, and reports to its hub.

pub mod git;
mod serve;

pub use serve::{Config, serve};
