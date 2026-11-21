//! What `RecordsFeed`, `OpenSession` and `OpenTerminal` share: following something a hub serves, on a thread of its
//! own, and telling readers when it changed. When the subscription falls behind, or there's a new connection, it starts
//! again from where it got to.

mod following;
#[cfg(all(test, unix))]
mod tests;

pub use following::FollowError;
pub(super) use following::{Follow, Giver, Reach, Source, Teller};
