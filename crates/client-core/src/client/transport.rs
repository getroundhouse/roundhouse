//! The byte stream under a connection: a Unix socket to a hub on this machine, or TLS over TCP to one elsewhere.
//! Either way, one thread reads while others write.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::os::unix::net::UnixStream;
use std::path::Path;

pub(super) struct Transport {
    /// Read by one thread, which blocks until the hub sends something.
    pub(super) reader: Box<dyn Read + Send>,
    /// Writes a whole frame, or fails once the hub has gone.
    pub(super) writer: Box<dyn Write + Send>,
    pub(super) closer: Closer,
}

impl Transport {
    pub(super) fn unix(socket: &Path) -> std::io::Result<Transport> {
        let stream = UnixStream::connect(socket)?;
        Ok(Transport {
            reader: Box::new(stream.try_clone()?),
            closer: Closer(Socket::Unix(stream.try_clone()?)),
            writer: Box::new(stream),
        })
    }
}

/// Shuts the socket down when dropped, which also ends the reader's wait on it.
pub(super) struct Closer(Socket);

enum Socket {
    Unix(UnixStream),
    Tcp(TcpStream),
}

impl Closer {
    pub(super) fn tcp(stream: TcpStream) -> Closer {
        Closer(Socket::Tcp(stream))
    }
}

impl Drop for Closer {
    fn drop(&mut self) {
        let _ = match &self.0 {
            Socket::Unix(stream) => stream.shutdown(Shutdown::Both),
            Socket::Tcp(stream) => stream.shutdown(Shutdown::Both),
        };
    }
}
