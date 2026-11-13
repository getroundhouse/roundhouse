//! Removing a worktree, as its workspace is archived, or another made for the workspace's work when its hub asks. A
//! build's output can be tens of thousands of files, which take seconds to delete, so the worktree is moved aside, and
//! deleted once it's forgotten.

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
pub(super) use deleting::remove_part_made;
pub use deleting::remove_workspace;
#[cfg(test)]
pub(crate) use deleting::{DELETING, LOCK};
#[cfg(test)]
use deleting::{Force, forget};
