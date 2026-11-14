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

mod recording;
pub use recording::{Initialized, Places, SessionChange, StatusChange, TurnEdge, cancelled_approval};
