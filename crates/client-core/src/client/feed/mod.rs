//! One hub's records, kept current by following its feed, so a client only reads them.

#[cfg(test)]
use crate::Agents;
#[cfg(test)]
use protocol::model::Records;

#[cfg(test)]
mod tests;

mod subscription;
pub use subscription::RecordsFeed;
