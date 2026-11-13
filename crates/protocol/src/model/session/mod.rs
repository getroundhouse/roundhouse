//! A chat's record: which agent it runs, where that agent has got to, and what the person has read.

#[cfg(test)]
use super::{CACHE_WORTH_TELLING_TOKENS, ContextUsage, PromptCache, Seq};
#[cfg(test)]
use crate::event::{AgentTask, TaskKind, TurnOutcome};

#[cfg(test)]
mod tests;

mod record;
pub use record::{AgentKind, OpenApproval, Session, SessionStatus};
