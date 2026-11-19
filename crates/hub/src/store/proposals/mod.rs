//! The proposals agents make, each in its chat. A proposal stays as long as its chat does.

#[cfg(test)]
mod tests;

mod proposal_rows;
pub(super) use proposal_rows::take_back_answer_on;
pub use proposal_rows::{AnswerToKeep, Answering, NewRevision};
