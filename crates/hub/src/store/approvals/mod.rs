//! The approvals each session's agent is waiting on, kept on the session's record.

#[cfg(test)]
use super::Store;
#[cfg(test)]
use protocol::model::Seq;

#[cfg(test)]
mod tests;

mod approval_rows;
pub(super) use approval_rows::{note_approvals, request_on};
