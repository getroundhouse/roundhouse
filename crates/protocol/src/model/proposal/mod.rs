//! A proposal: a decision too large for a question, which an agent writes with Roundhouse's tools as a structure, and
//! the person reads and answers in a tab of its own. The hub keeps it, as `docs/architecture.md` says under "State
//! ownership", so every client shows the same one.

#[cfg(test)]
mod tests;

mod checks;
mod record;
mod response;
mod written;

pub use checks::{MAX_HEADER_CHARS, MAX_OPTIONS, MAX_PROPOSAL_BYTES, MAX_QUESTIONS, MIN_OPTIONS, ProposalProblem};
pub use record::{Proposal, ProposalRevision, RevisionNumber};
pub use response::{AnswerProblem, Choice, MAX_NOTE_BYTES, Response, RevisionStanding};
pub use written::{OptionPlace, ProposalOption, ProposalQuestion, Proposed, Recommendation};
