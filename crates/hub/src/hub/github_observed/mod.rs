//! What's sent of GitHub as observations, worked out from what GitHub's cache keeps, as
//! `docs/principles/observations.md` has a value worked out from an observation be one: what GitHub says of each issue
//! and pull request a chip follows, at its own level, and of each workspace's own pull request, how it stands at the
//! hub's level, and its checks, history and stack, and whether GitHub showed its repository, at the workspace's. The
//! cache keeps what's read once, by what it describes, and this keeps each observation in the store as a kind of its
//! own, worked out again whenever what it's worked out from changes: a read the cache keeps, something it forgets, a
//! workspace's record, a read of a workspace's pull request that ends, or what clients follow. One task works them out,
//! in turn, so no value worked out from older state lands after one worked out from newer.
//!
//! Each is current while a client follows what it's of, from the first read after, until a read fails or the last
//! follower stops, and otherwise as of the read that found what it says. A chip's is gone once the cache forgets what
//! it's of, as it does what nothing has asked for in a week. A workspace's go with the workspace, and those of its
//! pull request once the cache forgets that pull request, or the record links to another.

mod chips;
mod listed;
mod observing;
mod reads;
#[cfg(test)]
mod tests;
mod workspaces;

pub(crate) use reads::ReadsEnded;
