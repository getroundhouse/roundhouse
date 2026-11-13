//! Making files and directories that only this user can open, as a hub's data directory and what's in it are, and the
//! sockets that only this user can connect to. The hub, `client-core`, `agents` and `worker` all make them, and none of
//! the four is one the other three may depend on, so this is its own small crate.
//!
//! Windows has no modes, so there each of these does no more than std does: what's created there takes its folder's
//! permissions, which under a user's profile, where the data directory is unless `ROUNDHOUSE_HOME` names another,
//! keep other users out.

#[cfg(test)]
mod tests;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::{create_dir_all, file_options, restrict};
#[cfg(windows)]
pub use windows::{create_dir_all, file_options, restrict};
