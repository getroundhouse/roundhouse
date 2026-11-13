//! A file as a commit held it, read a page of lines at a time, so one too big to send whole can still be shown. Each
//! read streams the file from git, keeping only the pages asked for, and stops once it has them. Nothing is kept
//! between reads, and each says in the worker's log how much it streamed and how long it took, so whether streaming
//! keeps up with the files people have can be read from there.

#[cfg(test)]
use super::MAX_TEXT_BYTES;

#[cfg(test)]
mod tests;

mod paging;
#[cfg(test)]
use paging::{LINE_BYTES, Limits, PAGE_LINES, Paging, Reads, Wanted};
pub(crate) use paging::{open_at, pages};
