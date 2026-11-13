//! The approvals still waiting on an answer: as the session's record says, with what's loaded since, or among what a
//! log holds, from a hub too old to say. A request the record names that no page loaded here holds is fetched on its
//! own, so what waits is asked about wherever in the log it was asked.

#[cfg(test)]
use super::{Position, SessionLog};
#[cfg(test)]
use protocol::event::{EventBody, SessionEvent};
#[cfg(test)]
use protocol::model::{ApprovalId, OpenApproval, Seq};

#[cfg(test)]
mod tests;

mod pending;
pub(super) use pending::Apart;
pub use pending::RecordedApprovals;
