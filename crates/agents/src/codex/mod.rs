//! Codex's persistent app-server, translated into Roundhouse's capability contracts.

mod approvals;
mod claims;
mod control;
mod description;
mod items;
mod launch;
mod notifications;
mod one_off;
mod rpc;
mod running;
mod state;
mod supervise;
mod terminals;
mod turn;

pub use description::description;
pub use one_off::{account, title_for};
pub use running::{spawn, spawn_at};

#[cfg(feature = "testing")]
pub use running::spawn_tapped;

#[cfg(all(feature = "testing", unix))]
pub mod conformance;

#[cfg(test)]
mod tests;
