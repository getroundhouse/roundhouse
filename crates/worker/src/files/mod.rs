//! A worktree's files, for a client to browse: tracked, new and ignored, what's in one of its folders, and what one
//! file holds, now or in a commit, whole or in parts.

mod pages;

pub(crate) use pages::{open_at, pages};

#[cfg(test)]
mod tests;

mod contents;
mod range;

#[cfg(test)]
use contents::{MAX_IMAGE_BYTES, bounded};
use contents::{MAX_TEXT_BYTES, SNIFF_BYTES, relative};
pub(crate) use contents::{list, read, read_at, read_folder};
pub(crate) use range::read_range;
