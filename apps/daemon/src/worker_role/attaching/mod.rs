//! The command a hub runs to reach a worker an earlier build left listening on a socket in its state directory, as
//! that build's local workers did to outlive their hub. Nothing starts one any more, since a worker that outlives its
//! hub dials it, and this goes at the next break of the worker protocol, as
//! `docs/decisions/hub-speaks-previous-version.md` says. Only Unix has the socket, so on Windows it refuses.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub(super) use unix::attach;
#[cfg(windows)]
pub(super) use windows::attach;
