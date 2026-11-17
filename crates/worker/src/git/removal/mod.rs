//! Removing a worktree, as its workspace is archived, or another made for the workspace's work when its hub asks. A
//! build's output can be tens of thousands of files, which take seconds to delete, so the worktree is moved aside, and
//! deleted once it's forgotten. One that can't be moved is refused, and left as it was, rather than deleted where it
//! is, since git forgets a worktree it was deleting even when it stops part way, leaving half of one.

#[cfg(test)]
use super::status;
#[cfg(test)]
use crate::deleting::Deleter;
#[cfg(test)]
use protocol::client::{ApiError, ErrorCode};
#[cfg(test)]
use std::fs::File;
#[cfg(test)]
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

mod deleting;
mod forgotten;
pub use deleting::remove_workspace;
#[cfg(test)]
pub(crate) use deleting::{DELETING, LOCK};
#[cfg(test)]
use deleting::{Force, forget};
pub(super) use deleting::{locked_part_made, remove_part_made};
