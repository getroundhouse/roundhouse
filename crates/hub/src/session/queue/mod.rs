//! The queue: prompts waiting for the agent to finish, each sent as a turn of its own.
//!
//! The queue is on the session's record, so it survives the hub and every client sees it. The first prompt in it is
//! sent once the agent is idle and has taken every prompt it was sent, since one sent before then would fold into the
//! turn the agent is about to start. It's held once the person stops a turn, since stopping means stopping, and once
//! the agent goes away mid-turn or fails its turn, since then it's for the person to say whether to go on, and a prompt
//! sent after a failure, such as a usage limit, could fail as the turn did. The hold is on the record too, so it lasts
//! through a restart, and a prompt the person sends lets it go.

#[cfg(test)]
use protocol::model::Seq;

#[cfg(test)]
mod tests;

mod delivery;

pub(super) use delivery::QueueWatch;
