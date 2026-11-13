//! Where a session's agent has got to, and what recording an event does to all of it, in one step.

#[cfg(test)]
use super::Origin;
#[cfg(test)]
use super::queue::QueueWatch;
#[cfg(test)]
use crate::store::SessionChange;
#[cfg(test)]
use protocol::event::EventBody;
#[cfg(test)]
use protocol::model::{RunId, Seq, SessionStatus};

#[cfg(test)]
mod tests;

mod advancing;
pub(crate) use advancing::Progress;
pub(super) use advancing::{Next, Step};
