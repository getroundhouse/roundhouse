//! Drives the user's installed `claude` CLI over its stream-json stdio protocol. This is the same
//! protocol the Agent SDK uses, so there is no sidecar. See `docs/claude-cli.md`.

mod effort;
mod normalize;
mod process;
mod title;

pub use effort::applied_effort;
pub use normalize::{Normalizer, Parsed};
pub use process::{SCRUBBED_ENV, build_args, spawn, spawn_at};
pub use title::title_for;
