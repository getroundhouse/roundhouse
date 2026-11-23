//! The feed: the hub's notices, for every client following it, leaving out any copy of a record that's no newer than
//! one it has told of, with what each moves, so a client that reads an observation in place of a record's field isn't
//! sent a copy that moves only that field.

#[cfg(test)]
mod tests;

mod fed;
mod subscription;
mod telling;

pub(crate) use fed::{Fed, Moves, Notices};
pub(super) use subscription::Feed;
