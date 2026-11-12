//! What a hub runs to reach a worker that can outlive it: a command that connects its own stdin and stdout to the
//! worker's socket, and starts the worker first if none is serving it. So the hub's end is a command's stdin and
//! stdout, as for any worker, and when the hub goes, only the command goes with it.

use std::io::{self, Write as _};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

/// How long a worker that was just started gets to serve its socket. It can first wait for one that's stopping to let
/// go of the instance, for up to its claim's wait.
const START_WAIT: Duration = Duration::from_secs(30);

/// Copies this process's stdin to the worker serving `socket`, and what the worker sends to its stdout, until the
/// worker closes the connection. Once stdin ends, the worker hears that nothing more is coming, and what it still
/// sends is copied on, such as what its agents said as they stopped. `start` starts a worker to serve the socket, and
/// is called if none is.
pub fn attach(socket: &Path, start: impl FnOnce() -> io::Result<()>) -> io::Result<()> {
    let stream = match UnixStream::connect(socket) {
        Ok(stream) => stream,
        Err(e) if unserved(&e) => {
            start()?;
            connect_within(socket, START_WAIT)?
        }
        Err(e) => return Err(e),
    };
    let mut to_worker = stream.try_clone()?;
    // On a thread of its own, since reading stdin blocks. It's left behind once the worker has closed the connection.
    std::thread::spawn(move || {
        let _ = io::copy(&mut io::stdin().lock(), &mut to_worker);
        let _ = to_worker.shutdown(Shutdown::Write);
    });
    let mut stdout = io::stdout().lock();
    io::copy(&mut &stream, &mut stdout)?;
    stdout.flush()
}

/// What connecting to a socket that nobody serves fails with: there's none, or one that a worker left behind.
fn unserved(e: &io::Error) -> bool {
    matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused)
}

fn connect_within(socket: &Path, wait: Duration) -> io::Result<UnixStream> {
    let deadline = Instant::now() + wait;
    loop {
        match UnixStream::connect(socket) {
            Err(e) if unserved(&e) && Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            connected => return connected,
        }
    }
}
