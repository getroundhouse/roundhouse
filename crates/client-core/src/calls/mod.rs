//! What a tool call is, and what its result says, for every client alike. The agent's adapter says what kind of call
//! each is, and a client reads the call's input knowing its kind, so a client draws kinds of call rather than tools. A
//! call recorded before adapters said is read from its tool's name, as [`older`] has Claude's of then. A tool with no
//! kind of its own, such as one from an MCP server, is `Other`, and still gets a readable line. [`ToolCall::summary`]
//! is the one line every client gives a call to say what it's about.

pub mod agent;
mod command;
mod failure;
pub mod files;
mod older;
mod other;
mod search;
pub mod web;

pub use agent::Agent;
pub use command::Command;
pub use failure::{Failure, ResultForm};
pub use files::{EditFile, ReadFile, WriteFile};
pub use other::Other;
pub use search::{FileLines, Found, Looks, Search};
pub use web::{Fetch, Link, Results, WebSearch};

#[cfg(test)]
mod tests;

mod recorded;
pub mod typed;
pub use recorded::{Recorded, ToolCall, asked_about, count, first_line};
use recorded::{description, main_input, str_field};
