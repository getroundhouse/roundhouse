//! Drives the user's installed `claude` CLI over its stream-json stdio protocol. This is the same
//! protocol the Agent SDK uses, so there is no sidecar. See `docs/claude-cli.md`.

mod account;
mod approvals;
mod claims;
mod context;
mod controls;
mod description;
mod effort;
mod kinds;
mod login;
mod login_check;
mod mode;
mod normalize;
mod one_off;
mod process;
mod title;
mod usage;

pub use account::account;
pub use context::{ModelWindows, context_usage};
pub use description::description;
pub use effort::applied_effort;
pub use normalize::{Normalized, Normalizer, Parsed};
pub use process::{SCRUBBED_ENV, build_args, spawn, spawn_at};
pub use title::title_for;
pub use usage::{lists_every_models_week, usage_limits};

mod server_name;
use server_name::TOOLS_SERVER;

#[cfg(all(feature = "testing", unix))]
pub mod conformance;

#[cfg(feature = "testing")]
pub use process::spawn_tapped;
