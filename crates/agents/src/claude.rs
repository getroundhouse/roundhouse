//! Drives the user's installed `claude` CLI over its stream-json stdio protocol. This is the same
//! protocol the Agent SDK uses, so there is no sidecar. See `docs/claude-cli.md`.

mod account;
mod context;
mod effort;
mod mode;
mod normalize;
mod one_off;
mod process;
mod title;
mod usage;

pub use account::account;
pub use context::context_usage;
pub use effort::applied_effort;
pub use normalize::{Normalized, Normalizer, Parsed};
pub use process::{SCRUBBED_ENV, build_args, spawn, spawn_at};
pub use title::title_for;
pub use usage::usage_limits;
