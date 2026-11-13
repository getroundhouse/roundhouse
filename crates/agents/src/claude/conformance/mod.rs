//! Claude's binding for [`crate::conformance`]: the scripted model's API and each scenario's script, how a scenario's
//! processes are launched on the real CLI, and how a capture of them is replayed through the adapter.
//! `docs/conformance.md` says what a trial is for, and how to play one.

mod fixture;
mod live;
pub mod messages;
mod replay;
mod scripts;
mod stream_json;

pub use live::{KEPT_ENV, Live, Options, find_cli, live};
pub use replay::replay;

mod launch_spec;
use launch_spec::{MODEL, project_folder, spec};
