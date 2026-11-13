//! What a subagent has done, for the call that started it. A chat that holds only the agent's own events draws it
//! from the subagent's record, and its work is in the subagent tab.
//!
//! From a hub that can't read the agent's own events apart, the chat holds every event, so it's gathered from the log
//! instead. Its events arrive in rows of their own, after the call and among the agent's other work and any other
//! subagent's, so the call gathers them and their own rows are empty. One too far after its call to be gathered is
//! drawn where it is instead, so nothing the subagent did goes missing.

mod lately;

mod calls;
pub(in crate::rows) use calls::Subagent;
pub(crate) use calls::{REACH, SubagentCalls, drawn_in_its_call, its_call};
