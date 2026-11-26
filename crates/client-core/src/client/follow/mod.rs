//! What `RecordsFeed`, `OpenSession` and `OpenTerminal` share: following something a hub serves, on a thread of its
//! own, and telling readers when it changed. When the subscription falls behind, or there's a new connection, it starts
//! again from where it got to.

mod following;
mod thread;

pub(crate) use following::Carried;
pub use following::FollowError;
#[cfg(test)]
pub(super) use following::Unstarted;
pub(super) use following::{Following, Giver, Reach, Source, Teller};

#[cfg(test)]
mod tests;
