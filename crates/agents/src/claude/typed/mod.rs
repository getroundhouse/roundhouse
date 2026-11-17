//! Each of the CLI's tools as the kind of call it makes, and what came back for it as what that kind sends back, which
//! the adapter records, and says of a request to make one, so that no client needs the CLI's names for its tools, the
//! shape of their inputs, or its words for what they sent back. A result holds what came back alone: how a command
//! exited, and the tags the CLI puts around a tool's own refusal, come out of its words.
//!
//! The hub has a copy of this, frozen, for what was recorded before the adapter typed its calls, which never changes,
//! and this one follows the CLI, so they aren't one table.

mod calls;
mod outputs;
#[cfg(test)]
mod tests;

pub(super) use calls::{PLAN, QUESTIONS, call};
pub(super) use outputs::say;
