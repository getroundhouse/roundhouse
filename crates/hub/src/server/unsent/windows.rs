//! On Windows, which has no option for what's unsent alone.

use std::io;

use tokio::net::TcpStream;

/// Sets nothing, since Windows holds about 64 KiB by itself, which at 1 Mbit/s keeps a reply under a flood to 0.65 s
/// behind a build's log and 1.4 s behind colour, as #2298 measured. Sizing `SO_SNDBUF` from the ideal send backlog
/// held no less, and a smaller fixed one would also turn off Windows' own sizing of the buffer.
#[expect(clippy::unnecessary_wraps, reason = "it takes the place of macOS's and Linux's, which can fail")]
pub(in crate::server) fn set_most_unsent(_stream: &TcpStream, _bytes: u32) -> io::Result<()> {
    Ok(())
}
