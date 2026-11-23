//! Who follows what: every subscription of every connection, by what it follows. It's the hub's one signal for what to
//! keep current, whichever request a subscription came by, as "What the hub keeps current" in
//! `docs/principles/serving.md` says.

mod followed;
mod list;
#[cfg(test)]
mod tests;

pub(crate) use followed::Followed;
pub(crate) use list::{Followers, Following};
