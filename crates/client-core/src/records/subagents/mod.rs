//! Each subagent's record, for the sessions a view is showing. They aren't among every record, since each holds what
//! its subagent was asked and its report in full, so a session's are listed once a view asks for them, and the feed
//! keeps them current from then on.
//!
//! No IO. As with every record, a list and the feed's notices can arrive in any order, and each subagent keeps
//! whichever copy has the newer revision, as [`protocol::model::Rev::supersedes`] says. The hub never removes a
//! subagent's record, so one goes only when no view wants its session any more.

#[cfg(test)]
mod tests;

mod calls;
pub(crate) use calls::Subagents;
