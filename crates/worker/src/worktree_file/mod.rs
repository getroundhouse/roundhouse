//! Finding a file or a folder a client names inside a worktree, and refusing a path that isn't written as a list of
//! the worktree's files writes one. A path can lead anywhere through `..` or a link, so it only counts as in the
//! worktree once every link on the way is followed.

#[cfg(test)]
mod tests;

mod paths;
pub(crate) use paths::{Last, find, find_folder, inside, relative};
