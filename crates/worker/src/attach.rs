//! What a hub runs to reach a worker an earlier build left listening on a socket, which doesn't dial: a command that
//! connects its own stdin and stdout to the worker's socket. So the hub's end is a command's stdin and stdout, and when
//! the hub goes, only the command goes with it. Nothing starts a worker that listens any more.
//!
//! It goes at the next break of the worker protocol, as `docs/decisions/hub-speaks-previous-version.md` says of what
//! the hub keeps for workers of an earlier build.

use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;

/// Copies this process's stdin to the worker serving `socket`, and what the worker sends to its stdout, until the
/// worker closes the connection. Once stdin ends, the worker hears that nothing more is coming, and what it still
/// sends is copied on, such as what its agents said as they stopped. Fails if no worker serves it.
pub fn attach(socket: &Path) -> io::Result<()> {
    let stream = UnixStream::connect(socket)?;
    let mut to_worker = stream.try_clone()?;
    // On a thread of its own, since reading stdin blocks. It's left behind once the worker has closed the connection.
    std::thread::spawn(move || {
        let _ = relay(&mut io::stdin().lock(), &mut to_worker);
        let _ = to_worker.shutdown(Shutdown::Write);
    });
    relay(&mut &stream, &mut io::stdout().lock())
}

/// Copies `from` to `to` until `from` ends, passing on each read as it comes. Not `io::copy`, which on Linux splices
/// when either end is a pipe, as the hub's ends are, and a splice from a socket into a pipe holds the pipe's lock while
/// it waits for more from the socket. The hub couldn't read the worker's reply already in the pipe until the worker
/// sent something else, and the worker sends nothing until the hub's next call.
fn relay(from: &mut impl Read, to: &mut impl Write) -> io::Result<()> {
    let mut buffer = vec![0; 64 * 1024];
    loop {
        match from.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(read) => {
                to.write_all(&buffer[..read])?;
                to.flush()?;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}
