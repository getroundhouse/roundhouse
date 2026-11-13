//! How far each session has been read, which says whether its newest reply is unread.

#[cfg(test)]
use super::{Store, StoreError};
#[cfg(test)]
use protocol::model::Seq;

#[cfg(test)]
mod tests;

mod read_marks;
pub use read_marks::Reading;
pub(super) use read_marks::session_on;
