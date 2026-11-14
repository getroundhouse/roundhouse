//! The session event log, which is the record every client reads.
//!
//! Events are normalized so that clients don't parse agent formats. The worker also captures each
//! agent's raw output line, and the hub stores it beside the event, so that this view can be
//! rebuilt if the normalization turns out to be wrong.

pub use approval::{ApprovalDecision, ApprovalOutcome, secret_questions};
pub use asking::{Asking, Question, QuestionAnswer, QuestionOption};
pub use call::CallKind;
pub use compaction::{CompactedBy, CompactionOutcome};
pub use content::{ContentBlock, Denied, LinkedImage};
pub use ending::{ExitReason, SubagentOutcome, TurnOutcome};
pub use failure::{FailureKind, RequestFailure};
pub use file_change::{FileChange, Hunk, HunkLine, LineMark};
pub use task::{AgentTask, TaskKind};
pub use tokens::TokenUsage;

mod approval;
mod asking;
mod call;
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
pub use recorded::{AgentRunState, EventBody, NoticeLevel, SessionEvent};
