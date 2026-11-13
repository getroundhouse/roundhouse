//! A worktree's branches, read with git: which is checked out, and each branch the worktree has had checked out, with
//! its newest commit and where it's pushed. And where git writes them, which [`crate::branch_watch`] watches to tell
//! when to read them again.

#[cfg(test)]
use protocol::model::Branches;
#[cfg(test)]
use std::path::PathBuf;

#[cfg(test)]
pub(crate) mod tests;

mod watching;

#[cfg(test)]
use watching::left;
pub(crate) use watching::{Places, Read, checked_out, matters, places, read, read_now, watched};
