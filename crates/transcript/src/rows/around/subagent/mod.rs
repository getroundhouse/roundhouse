//! What a subagent has done, for the call that started it. A chat holds only the agent's own events, so it's drawn
//! from the subagent's record, and its work is in the subagent tab.

mod lately;
mod named;
pub(in crate::rows) use named::Names;

mod calls;
pub(in crate::rows) use calls::Subagent;
