//! The streaming tail: text the agent is still writing, which isn't in the log yet.
//!
//! Deltas aren't stored, because the finished message arrives as a durable event and replaces
//! them. A client that attaches mid-message gets a [`Tail`] snapshot and then deltas, so it sees
//! the partial text straight away. The hub and clients apply the same rules through this type, so
//! they can't disagree about what the tail contains.

#[cfg(test)]
use crate::event::EventBody;
#[cfg(test)]
use crate::model::{MessageId, RunId, Seq};

#[cfg(test)]
mod carrying_on;
#[cfg(test)]
mod compacting;
#[cfg(test)]
mod laws;

#[cfg(test)]
mod tests;

mod stream;
pub use stream::{BlockKind, Compacting, DeltaChange, InBackground, StreamDelta, Tail, TailBlock, TailMessage};
