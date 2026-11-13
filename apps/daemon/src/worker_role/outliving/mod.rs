//! A worker that can outlive its hub, which listens on a socket in its instance's directory, and the command a hub runs
//! to reach it. Only Unix has the socket, so on Windows both refuse.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub(super) use unix::{attach, listen};
#[cfg(windows)]
pub(super) use windows::{attach, listen};
