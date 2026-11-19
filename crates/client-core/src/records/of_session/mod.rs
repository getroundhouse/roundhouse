//! The records a session has many of, which aren't among every record, since each holds a lot: what a subagent was
//! asked and its report, or a proposal as its agent wrote it, in full. So a session's are listed once a view asks for
//! them, and the feed keeps them current from then on. Each kind is kept the same way, by a [`BySession`] of its own.
//!
//! No IO. As with every record, a list and the feed's notices can arrive in any order, and each keeps whichever copy
//! has the newer revision, as [`protocol::model::Rev::supersedes`] says. The hub never removes one while its session
//! is there, so one goes only when no view wants its session any more.

#[cfg(test)]
mod tests;

mod held;
mod kinds;
pub(crate) use held::BySession;
pub(crate) use kinds::OfSession;

mod named;
pub use named::subagent_named;
