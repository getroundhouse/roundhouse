//! Feeding a subscription to what GitHub says of what a client shows: what the hub knows of each, sent as it changes,
//! and again whole after the client says it shows something else.

mod following;
pub(super) use following::{Followed, follow};
mod pumping;
#[cfg(test)]
mod tests;
