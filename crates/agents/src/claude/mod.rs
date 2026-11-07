//! Drives the user's installed `claude` CLI over its stream-json stdio protocol. This is the same
//! protocol the Agent SDK uses, so there is no sidecar. See `docs/claude-cli.md`.

mod normalize;
mod process;

pub use normalize::{Parsed, parse_line};
pub use process::{SCRUBBED_ENV, build_args, spawn, spawn_at, write_pid_file};
