//! On Windows, which has no option for what's unsent alone.

use std::io;

use tokio::net::TcpStream;

/// Sets nothing yet, so Windows sizes the send buffer by itself. *Not built:* setting `SO_SNDBUF` from the ideal send
/// backlog Windows reports for the connection, as #2298 says.
pub(in crate::server) fn hold_little_unsent(_stream: &TcpStream) -> io::Result<()> {
    Ok(())
}
