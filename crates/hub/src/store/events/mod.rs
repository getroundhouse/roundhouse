//! Each session's event log.

#[cfg(test)]
use super::{Store, lock};
#[cfg(test)]
use protocol::event::{ApprovalOutcome, EventBody, SessionEvent};
#[cfg(test)]
use protocol::model::{Place, RunId, Seq, ToolUseId};
#[cfg(test)]
use rusqlite::Connection;

#[cfg(test)]
pub(crate) mod tests;

mod change;
mod recording;
pub use change::{Initialized, SessionChange, StatusChange, TurnEdge};
pub use recording::{Places, cancelled_approval};
