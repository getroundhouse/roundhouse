//! What a request to use a tool says of the use, read once for every client: the tool, what the use is for, what the
//! tool was given, why the agent asks, and which subagent asks. A client words and draws it, and reads nothing of the
//! agent's tools itself. It's answered by [`ToolUseAnswer`]s, which every client shows in the same order, and a draft
//! keeps the one chosen by name.

#[cfg(test)]
mod tests;

mod question;
pub use question::{Given, ToolUseAnswer, ToolUseAsked};
