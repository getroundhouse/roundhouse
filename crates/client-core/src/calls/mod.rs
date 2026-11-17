//! What a tool call is, and what its result says, for every client alike. The agent's adapter types each call and
//! what came back for it, so a client draws kinds of call rather than tools, and never reads an agent's names for its
//! tools or its words for what they sent back. A tool with no kind of its own, such as one from an MCP server, is
//! `Other`, and still gets a readable line. [`ToolCall::summary`] is the one line every client gives a call to say what
//! it's about.

pub mod agent;
mod command;
mod failure;
pub mod files;
mod other;
mod plain;
mod returned;
mod search;
pub mod subagents;
pub mod web;

pub use agent::Agent;
pub use command::{Command, Only};
pub use failure::{Ending, Failure, ResultForm};
pub use files::{EditFile, ReadFile, WriteFile};
pub use other::Other;
pub use returned::{Returned, in_the_background};
pub use search::{FileLines, Found, Looks, Search};
pub use subagents::ToSubagents;
pub use web::{Fetch, Link, Results, WebSearch};

#[cfg(test)]
mod tests;

mod recorded;
pub mod typed;
pub use recorded::{Recorded, ToolCall, count, first_line};
use recorded::{description, main_input};
