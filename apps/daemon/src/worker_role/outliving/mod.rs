//! A worker that listens on a socket in its state directory, as an earlier build's local workers did to outlive their
//! hub, and the command a hub runs to reach one. Nothing starts one any more, since a worker that outlives its hub
//! dials it. Only Unix has the socket, so on Windows both refuse.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub(super) use unix::{attach, listen};
#[cfg(windows)]
pub(super) use windows::{attach, listen};
