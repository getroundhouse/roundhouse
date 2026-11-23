//! What's sent of GitHub as observations, worked out from what GitHub's cache keeps, as
//! `docs/principles/observations.md` has a value worked out from an observation be one: for now, what GitHub says of
//! each issue and pull request a chip follows. The cache keeps what's read once, by what it describes, and this keeps
//! each observation in the store as a kind of its own, worked out again whenever what it's worked out from changes: a
//! read the cache keeps, something it forgets, or what clients follow. One task works them out, in turn, so no value
//! worked out from older state lands after one worked out from newer.
//!
//! An observation of a chip is current while a client follows it, from the first read after, until a read fails or
//! the last follower stops, and otherwise as of the read that found what it says. It's gone once the cache forgets
//! what it's of, as it does what nothing has asked for in a week.

mod listed;
mod observing;
#[cfg(test)]
mod tests;
