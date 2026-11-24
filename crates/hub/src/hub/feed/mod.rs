//! The feed: the hub's notices, for every client following it, leaving out any copy of a record that's no newer than
//! one it has told of.

#[cfg(test)]
mod tests;

mod fed;
mod subscription;
mod telling;

pub(crate) use fed::Notices;
pub(super) use subscription::Feed;
