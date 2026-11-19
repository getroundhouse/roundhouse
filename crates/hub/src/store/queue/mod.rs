//! Prompts waiting their turn, kept on their session's record.

#[cfg(test)]
use super::{Reading, Store, lock};

#[cfg(test)]
mod tests;

mod queued_rows;
pub use queued_rows::{Changed, Queued, Queuing, Unqueued};
pub(super) use queued_rows::{queue_on, take_out_recorded};
