//! Feeding a subscription to what GitHub says of what a client shows: what the hub knows of each, sent as it changes,
//! and again whole after the client says it shows something else.

mod following;
pub(super) use following::follow;
mod pumping;
pub(super) use pumping::pump;
#[cfg(test)]
mod tests;
