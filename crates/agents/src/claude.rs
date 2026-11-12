//! Drives the user's installed `claude` CLI over its stream-json stdio protocol. This is the same
//! protocol the Agent SDK uses, so there is no sidecar. See `docs/claude-cli.md`.

mod account;
mod approvals;
mod context;
mod description;
mod effort;
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
pub use usage::usage_limits;

/// The name the adapter gives the server of the worker's tools in the CLI's config, which the CLI puts in the name of
/// each of them, such as
/// `mcp__roundhouse__get_pull_request`, and in the description of a task one of them answers with, such as
/// `roundhouse/wait_for_pull_request`.
const TOOLS_SERVER: &str = "roundhouse";
