//! A workspace's diff as git writes it, read into the files it changes, each with the hunks git gives of it. Lines are
//! numbered from 1 on each side, as git numbers them. Git leaves out what's far from the changes, so the file tab
//! fills that in from the file as it was in the diff's commit.

mod parse;
mod paths;
#[cfg(test)]
mod tests;

pub use parse::{PatchError, Problem, parse};

mod lines;
pub use lines::{FilePatch, FileStatus, Hunk, LineKind, PatchLine, Unchanged};
