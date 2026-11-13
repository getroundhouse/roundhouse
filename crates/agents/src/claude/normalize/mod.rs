//! Turns one line of Claude's stdout into a normalized event.
//!
//! This has to survive the CLI changing underneath it, so it reads loosely typed JSON and never
//! fails. Anything it doesn't recognize still reaches the log as `AgentOther` with the raw line.

pub(super) use limits::{whole_percent, window_named};
pub(crate) use prompt::PromptProgress;
#[cfg(test)]
use protocol::event::EventBody;
#[cfg(test)]
use protocol::tail::{BlockKind, DeltaChange, StreamDelta};
#[cfg(test)]
use serde_json::Value;

mod advisor;
mod blocks;
mod calls;
mod compaction;
mod denied;
mod file_change;
mod images;
mod limits;
mod pending;
mod prompt;
mod request;
mod result_text;
mod said;
mod subagents;
mod surrogates;
mod tasks;
#[cfg(test)]
mod tests;
mod turn_end;
mod user;

mod normalizer;
#[cfg(test)]
use normalizer::parse_line;
pub(crate) use normalizer::value_of;
pub use normalizer::{Normalized, Normalizer, Parsed};
use normalizer::{each_string, id_of, other, str_of, string_of};
