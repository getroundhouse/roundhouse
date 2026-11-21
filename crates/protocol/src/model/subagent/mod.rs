//! A subagent's record: what it was asked, where it's got to, what it's done lately and what it reported, which the hub
//! keeps as it records the events that say, so a client can show a subagent without reading its work.

mod lately;
mod record;
mod recorded;
#[cfg(test)]
mod tests;

pub use record::{Lately, LatelyCall, LatelyResult, Said, Subagent, SubagentStatus};
pub use recorded::TouchedSubagents;
