//! One file's diff against a worktree's base, as [`protocol::model::FileDiff`] says, read a page at a time, for a
//! client showing the file that a change summary names.

mod diffing;
mod finding;
mod pages;
#[cfg(test)]
mod tests;

pub(crate) use diffing::diff_file;
