//! What a client holds of one session: a window of its event log, or of the events from one agent or subagent in
//! it, never all of it.
//!
//! No IO and no UI. A view asks for rows by index, this says what to fetch, and whoever owns the
//! connection does the fetching. Rows are indexed by each event's [`Position`], as [`Position::row`] says, so an index
//! means the same thing however much is loaded. An event's position is its `seq` in the whole log, and its place in the
//! events from one agent or subagent. The tail comes after the rows, with the prompts the agent hasn't read yet,
//! recorded or still on their way, as [`PromptAt`] says.

#[cfg(test)]
use protocol::event::{EventBody, SessionEvent};
#[cfg(test)]
use protocol::model::{QueuedPrompt, Seq};
#[cfg(test)]
use protocol::tail::StreamDelta;
#[cfg(test)]
use std::ops::Range;

mod approvals;
mod outline;
mod pages;
mod position;
mod prompts;
mod restart;

pub use approvals::RecordedApprovals;
use pages::before_live;
pub use position::Position;
pub use prompts::{PromptAt, PromptState, Settled};

#[cfg(test)]
mod laws;

#[cfg(test)]
mod tests;

mod log;
pub use log::{By, Changes, MAX_LOADED_EVENTS, Row, SessionLog};
