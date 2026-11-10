//! The bytes between a client and a hub: a Unix socket, or on Windows a named pipe. One thread reads while others
//! write, each through a clone of its own, and shutting any clone down ends them all.

#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub(super) use std::os::unix::net::UnixStream as Stream;
#[cfg(windows)]
pub(super) use windows::Stream;
