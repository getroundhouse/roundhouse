//! What an approval asks of the person, for a client to draw, the decisions that answer it, and what was answered, for
//! a client to say. A request says what it asks, typed, as its adapter read it, and [`ToolUseAsked`] says what one that
//! asks to make a call says of it. No client reads an agent's tools.

#[cfg(test)]
mod tests;
mod tool_use;

pub use tool_use::{Given, ToolUseAnswer, ToolUseAsked};

mod answering;
pub use answering::{Answered, Approval, claims_of_the_asker};
