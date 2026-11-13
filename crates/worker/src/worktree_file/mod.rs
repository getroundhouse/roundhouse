//! Finding a file or a folder a client names inside a worktree. A path can lead anywhere through `..` or a link, so it
//! only counts as in the worktree once every link on the way is followed.

#[cfg(test)]
mod tests;

mod paths;
pub(crate) use paths::{find, find_folder};
