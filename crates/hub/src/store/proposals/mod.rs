//! The proposals agents make, each in its chat. A proposal stays as long as its chat does.

#[cfg(test)]
mod tests;

mod proposal_reads;
mod proposal_rows;
pub use proposal_rows::{AnswerToKeep, Answering, NewRevision, Revising, Withdrawing};
pub(super) use proposal_rows::{proposals_of, take_back_answer_on};
