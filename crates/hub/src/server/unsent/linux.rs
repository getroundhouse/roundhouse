//! On Linux, with `TCP_NOTSENT_LOWAT`, which socket2 sets.

use std::io;

use socket2::SockRef;
use tokio::net::TcpStream;

/// Has the kernel take more of what's written only while less than `bytes` of it is still to be sent. What's sent and
/// not yet acknowledged doesn't count, so a fast link is kept as full as before.
pub(in crate::server) fn set_most_unsent(stream: &TcpStream, bytes: u32) -> io::Result<()> {
    SockRef::from(stream).set_tcp_notsent_lowat(bytes)
}

/// What [`set_most_unsent`] set.
#[cfg(test)]
pub(in crate::server) fn most_unsent(stream: &TcpStream) -> io::Result<u32> {
    SockRef::from(stream).tcp_notsent_lowat()
}
