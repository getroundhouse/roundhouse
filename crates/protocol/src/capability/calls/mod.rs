//! Tool calls: what each does, typed by its kind, and what each sent back, so a client draws kinds of call rather than
//! any agent's tools.
//!
//! A call is recorded as the [`Call`] its adapter read from the agent's tool and input, in place of that input, which
//! the raw line keeps, so a write's content is stored once. A tool with no kind of its own keeps the agent's name for
//! it and its input, as [`Call::Other`] and [`Call::Mcp`] do. A result is recorded as the [`CallOutput`] its kind sends
//! back, in place of the words the agent was told, which the raw line keeps too. Where agents give the same kind of
//! call different shapes, the adapter gives it this one: Claude's `Edit`, opencode's `edit` and each file of a Codex
//! patch are all edits of a file.

pub mod call;
mod claims;
mod kind;
pub mod output;

pub use call::Call;
pub use claims::{Capability, Claims};
pub use kind::CallKind;
pub use output::CallOutput;

#[cfg(test)]
mod tests;
