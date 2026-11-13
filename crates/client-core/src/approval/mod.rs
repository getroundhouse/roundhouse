//! What an approval asks of the person, for a client to draw, the decisions that answer it, in the forms the adapter
//! that asked takes, and what was answered, for a client to say. A request says what it asks, as its adapter read it,
//! and [`ToolUseAsked`] says what one that asks to use a tool says of the use.
//! One recorded before adapters said is read from its tool and its input by [`older`], which holds the forms an
//! adapter from before takes as well, so nothing else in a client reads an agent's tools.

pub(crate) mod older;
#[cfg(test)]
mod tests;
mod tool_use;

pub use tool_use::{Given, ToolUseAnswer, ToolUseAsked};

mod answering;
pub use answering::{Answered, Approval};
