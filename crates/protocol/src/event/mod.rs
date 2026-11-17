//! The session event log, which is the record every client reads.
//!
//! Events are normalized so that clients don't parse agent formats. The worker also captures each
//! agent's raw output line, and the hub stores it beside the event, so that this view can be
//! rebuilt if the normalization turns out to be wrong.

pub use approval::{ApprovalDecision, ApprovalOutcome, secret_questions};
pub use asking::{Asking, Question, QuestionOption};
pub use compaction::{CompactedBy, CompactionOutcome};
pub use content::{ContentBlock, Denied, LinkedImage};
pub use ending::{ExitReason, TaskOutcome, TurnOutcome};
pub use failure::{Again, FailureKind, RequestFailure};
pub use file_change::{FileChange, Hunk, HunkLine, LineMark};
pub use task::{AgentTask, EndedTask, TaskChange, TaskKind};
pub use tokens::TokenUsage;

mod approval;
mod asking;
mod claims;
mod compaction;
mod content;
mod ending;
mod failure;
mod file_change;
mod filing;
mod task;
mod tokens;

mod recorded;
#[cfg(test)]
mod tests;
pub use recorded::{AgentRunState, EventBody, NoticeLevel, SessionEvent};
