//! On macOS and Linux, with `TCP_NOTSENT_LOWAT`.

use std::io;
use std::os::fd::AsRawFd as _;

use tokio::net::TcpStream;

/// The most of what's written that the kernel holds unsent, which a reply written now waits behind on top of the
/// frame being written and what's on the link. Small enough for a reply to wait about a tenth of a second for it at
/// 1 Mbit/s, and enough that the kernel never runs out of what to send while the hub writes the next frame.
pub(in crate::server) const MOST_UNSENT: u32 = 16 * 1024;

#[cfg(target_os = "linux")]
const TCP_NOTSENT_LOWAT: libc::c_int = libc::TCP_NOTSENT_LOWAT;
/// As `<netinet/tcp.h>` defines it on macOS, where `libc` doesn't name it.
#[cfg(target_os = "macos")]
const TCP_NOTSENT_LOWAT: libc::c_int = 0x201;

/// Has the kernel take more of what's written only while less than [`MOST_UNSENT`] of it is still to be sent. What's
/// sent and not yet acknowledged doesn't count, so a fast link is kept as full as before.
pub(in crate::server) fn hold_little_unsent(stream: &TcpStream) -> io::Result<()> {
    let most = MOST_UNSENT.cast_signed();
    let length = libc::socklen_t::try_from(size_of::<libc::c_int>()).map_err(io::Error::other)?;
    // SAFETY: the descriptor is the stream's, open while it's borrowed, and the value is an int that outlives the
    // call, of the length given.
    let set = unsafe {
        libc::setsockopt(stream.as_raw_fd(), libc::IPPROTO_TCP, TCP_NOTSENT_LOWAT, (&raw const most).cast(), length)
    };
    if set == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

/// What the kernel holds unsent at most for the stream, as [`hold_little_unsent`] sets it.
#[cfg(test)]
pub(in crate::server) fn unsent_held(stream: &TcpStream) -> io::Result<u32> {
    let mut held: libc::c_int = 0;
    let mut length = libc::socklen_t::try_from(size_of::<libc::c_int>()).map_err(io::Error::other)?;
    // SAFETY: the descriptor is the stream's, open while it's borrowed, and the value and its length are ints that
    // outlive the call, which writes at most the length given.
    let got = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::IPPROTO_TCP,
            TCP_NOTSENT_LOWAT,
            (&raw mut held).cast(),
            &raw mut length,
        )
    };
    if got == 0 { Ok(held.cast_unsigned()) } else { Err(io::Error::last_os_error()) }
}
