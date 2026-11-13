//! Each subagent's record, changed in the transaction that records the event that changes it.

#[cfg(test)]
use super::Store;
#[cfg(test)]
use protocol::event::{ContentBlock, EventBody, SessionEvent};
#[cfg(test)]
use protocol::model::{RecentWork, Rev, Subagent, SubagentStatus, ToolUseId};

mod recent;

#[cfg(test)]
mod tests;

mod subagent_rows;

pub(super) use subagent_rows::note_subagents;
