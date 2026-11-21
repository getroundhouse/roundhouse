//! Codex's persistent app-server, translated into Roundhouse's capability contracts.

mod approvals;
mod catalog;
mod claims;
mod control;
mod description;
mod items;
mod launch;
mod notices;
mod notifications;
mod one_off;
mod rpc;
mod running;
mod state;
mod subagents;
mod supervise;
mod terminals;
mod turn;
mod turn_reports;
mod usage;

pub use catalog::models_for;
pub use description::description;
pub use one_off::{account, title_for};
pub use running::{spawn, spawn_at};

#[cfg(any(test, feature = "testing"))]
pub use running::spawn_tapped;

#[cfg(all(any(test, feature = "testing"), unix))]
pub mod conformance;

#[cfg(test)]
mod tests;
