//! On macOS, with `TCP_NOTSENT_LOWAT`, which neither socket2 nor `libc` has there, so it's set through `setsockopt`.

use std::io;
use std::os::fd::AsRawFd as _;

use tokio::net::TcpStream;

/// As `<netinet/tcp.h>` defines it.
const TCP_NOTSENT_LOWAT: libc::c_int = 0x201;

/// Has the kernel take more of what's written only while less than `bytes` of it is still to be sent. What's sent and
/// not yet acknowledged doesn't count, so a fast link is kept as full as before.
pub(in crate::server) fn set_most_unsent(stream: &TcpStream, bytes: u32) -> io::Result<()> {
    let most = libc::c_int::try_from(bytes).map_err(io::Error::other)?;
    let length = libc::socklen_t::try_from(size_of::<libc::c_int>()).map_err(io::Error::other)?;
    // SAFETY: the descriptor is the stream's, open while it's borrowed, and the value is an int that outlives the
    // call, of the length given.
    let set = unsafe {
        libc::setsockopt(stream.as_raw_fd(), libc::IPPROTO_TCP, TCP_NOTSENT_LOWAT, (&raw const most).cast(), length)
    };
    if set == 0 { Ok(()) } else { Err(io::Error::last_os_error()) }
}

/// What [`set_most_unsent`] set.
#[cfg(test)]
pub(in crate::server) fn most_unsent(stream: &TcpStream) -> io::Result<u32> {
    let mut most: libc::c_int = 0;
    let mut length = libc::socklen_t::try_from(size_of::<libc::c_int>()).map_err(io::Error::other)?;
    // SAFETY: the descriptor is the stream's, open while it's borrowed, and the value and its length outlive the call,
    // which writes at most the length given.
    let got = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::IPPROTO_TCP,
            TCP_NOTSENT_LOWAT,
            (&raw mut most).cast(),
            &raw mut length,
        )
    };
    if got == 0 { u32::try_from(most).map_err(io::Error::other) } else { Err(io::Error::last_os_error()) }
}
