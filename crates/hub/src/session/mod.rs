//! One session, while the hub has it in memory.
//!
//! Two tasks do the work. `ingest` records what the agent said, and `commands` carries out what
//! clients ask for, which means waiting on worker replies. They're separate because those replies
//! arrive on the same stream as agent output: if one task did both, waiting for a reply could stop
//! the output being read, and the reply behind it would never arrive.

#[cfg(test)]
use protocol::event::EventBody;
#[cfg(test)]
use protocol::model::{ContextUsage, RunId, Seq, SessionStatus};

mod cache;
mod commands;
mod controls;
mod decisions;
mod ingest;
mod limits;
mod messages;
mod pickup;
mod previous;
mod progress;
mod queue;
mod record;
mod status;
#[cfg(test)]
mod tests;
mod waits;

pub(crate) use messages::{Command, Item};
#[cfg(test)]
pub(crate) use pickup::PICKS_UP_WITHIN;
pub(crate) use progress::Progress;

mod live;

use live::{AgentRun, LiveState, publish};
pub(crate) use live::{CarriedOver, Live, LiveItem, LiveSession, Origin};
