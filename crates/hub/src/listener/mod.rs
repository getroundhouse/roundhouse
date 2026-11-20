//! Where clients connect: a Unix socket, or on Windows a named pipe, since neither tokio nor stable Rust has Unix
//! sockets there. Either way, only this user can send the hub anything, which is what authenticates a local client.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub(crate) use unix::{Listener, unbind};
#[cfg(windows)]
pub(crate) use windows::{Listener, unbind};
