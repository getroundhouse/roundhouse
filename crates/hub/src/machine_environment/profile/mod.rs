//! The environment the user's profile gives, which each platform reads its own way: Unix from the login shell, and
//! Windows from the registry.

mod entries;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as sys;
#[cfg(windows)]
use windows as sys;

#[cfg(test)]
pub(super) use entries::{from_block, from_shell};
pub(super) use sys::read;
#[cfg(all(test, unix))]
pub(super) use unix::read_until_second_marker;
