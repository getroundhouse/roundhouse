//! What the hub's feed says: that a record or an observation changed, as a copy of it or its removal, or that a client
//! missed some.

#[cfg(test)]
mod tests;

mod notices;
pub use notices::{HubNotice, ObservationNotice, RecordNotice};
