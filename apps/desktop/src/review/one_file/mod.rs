//! The file the file tab shows, read with one file's diff while the hub keeps the workspace's changed files current,
//! and read again only when the summary gives it another version, base, status or old path, or its read failed, so
//! the whole diff is never read for it. What was read of it last shows until the next read is taken.

mod file;
mod pages;
mod reading;

pub(super) use file::{OneFile, Taken};
