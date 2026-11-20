//! A proposal: a decision too large for a question, which an agent writes with Roundhouse's tools as a structure, and
//! the person reads and answers in a tab of its own. The hub keeps it, so every client shows the same one, as
//! `docs/principles/state.md` says.

#[cfg(test)]
mod tests;

mod checks;
mod record;
mod response;
mod summary;
mod written;

pub use checks::{MAX_HEADER_CHARS, MAX_OPTIONS, MAX_PROPOSAL_BYTES, MAX_QUESTIONS, MIN_OPTIONS, ProposalProblem};
pub use record::{Proposal, ProposalRevision, RevisionNumber, Withdrawal};
pub use response::{AnswerProblem, Choice, MAX_NOTE_BYTES, Response, RevisionStanding, Round};
pub use summary::{ChosenOption, NOTE_START_CHARS, ResponseSummary, RevisionSummary, chosen_in_words};
pub use written::{OptionLetter, OptionPlace, ProposalOption, ProposalQuestion, Proposed, Recommendation};
