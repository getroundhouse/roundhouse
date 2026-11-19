//! The worker's clones of the repositories it's told of by their address, which it makes their worktrees from, as it
//! does from a checkout. One for each address, in the clones directory its provider names, which every worker on the
//! machine that the provider runs shares: made the first time a workspace needs it, kept, and fetched from then on, as
//! a checkout is, before each workspace is made from it.
//!
//! A clone is bare, since nothing works in it, and set up as `git clone` sets one up but for that: with `origin`'s
//! branches and tags as last fetched, its `HEAD`, and no branch of its own until a workspace makes one. Each is made in
//! a folder where nothing looks for a clone, and moved into place once it's whole, so one that a stop cut short is
//! never taken for whole, and the next worker to want it clears it and starts again. Workers take turns at making each,
//! by a lock beside it in that folder, so the second to want one waits for the first, and then takes the one it made.

mod archive;
#[cfg(test)]
mod archive_tests;
mod making;
mod place;
#[cfg(test)]
mod tests;

pub(crate) use archive::{capture_archived, restore_archived};
pub(crate) use place::Clones;
