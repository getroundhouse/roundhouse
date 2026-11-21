//! Drives the user's installed `agy` CLI over ACP 1.0 (`agy acp --disable-telemetry`). See `docs/reference/antigravity-cli.md`.

mod approvals;
mod async_agents;
mod calls;
mod claims;
mod compact;
mod control;
mod description;
mod files;
mod launch;
mod notifications;
mod one_off;
mod running;
mod state;
mod supervise;
mod swap;
mod turn;
mod wakeup;

pub use description::description;
pub use one_off::{account, title_for};
pub use running::{spawn, spawn_at};

#[cfg(any(test, feature = "testing"))]
pub use running::spawn_tapped;

#[cfg(all(any(test, feature = "testing"), unix))]
pub mod conformance;

#[cfg(test)]
mod tests;
