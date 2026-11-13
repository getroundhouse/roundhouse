//! Carrying on with what an earlier hub left at work. It left its workers running, so this hub connects to each again,
//! and each session whose agent is still running carries on with it, with its open approvals, what it runs in the
//! background, and the prompts the agent hasn't taken yet. A session whose agent has gone is interrupted, as one whose
//! worker went away is, and so is each session on an instance with no worker running, or one that can't be reached,
//! such as one that's stopping.

#[cfg(test)]
use protocol::event::EventBody;
#[cfg(test)]
use protocol::model::{RunId, Seq, SessionId};

#[cfg(test)]
mod tests;

mod recovery;
pub(crate) use recovery::LeftAtWork;
#[cfg(test)]
use recovery::interrupt;
