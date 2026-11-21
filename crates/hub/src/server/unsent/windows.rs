//! On Windows, which has no option for what's unsent alone.

use std::io;

use tokio::net::TcpStream;

/// Sets nothing yet, so Windows sizes the send buffer by itself. *Not built:* setting `SO_SNDBUF` from the ideal send
/// backlog Windows reports for the connection, as #2298 says.
#[expect(clippy::unnecessary_wraps, reason = "it takes the place of macOS's and Linux's, which can fail")]
pub(in crate::server) fn set_most_unsent(_stream: &TcpStream, _bytes: u32) -> io::Result<()> {
    Ok(())
}
