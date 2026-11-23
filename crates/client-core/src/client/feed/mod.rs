//! One hub's records, kept current by following the hub and by taking in what replies carry, so a client only reads
//! them.

#[cfg(test)]
use crate::Agents;
#[cfg(test)]
use protocol::model::Records;

#[cfg(test)]
mod tests;

mod folding;
mod subscription;
pub(super) use folding::{Folding, Reaches};
pub use subscription::RecordsFeed;
