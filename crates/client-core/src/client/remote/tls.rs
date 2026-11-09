//! TLS over TCP, blocking. rustls keeps a connection's state in one object, and a client reads on one thread while
//! writing from others, so the state is shared under a lock that's never held while waiting on the network: the
//! reader waits on its own handle to the socket, then takes the lock to decrypt what arrived.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection};
use socket2::{SockRef, TcpKeepalive};

use super::pin::PinnedKey;
use crate::client::transport::{Closer, Transport};

/// How long the handshake may take once connected. Only a hub that's very busy, or not a hub, takes longer.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// TLS records are at most this much plus a little, so one read usually brings a whole one.
const READ_BUFFER_BYTES: usize = 32 * 1024;
/// A phone keeps a connection open for minutes while someone watches a chat, and the hub's Mac can go to sleep, or
/// the phone change networks, without it closing. Nothing is sent while nothing changes, so nothing would notice,
/// and the chat would just stop moving. Probing after 10 s of quiet, every 5 s, and giving up after 3 unanswered
/// probes ends the connection within about 25 s of the hub going, so the app can say so and connect again.
const KEEPALIVE: TcpKeepalive =
    TcpKeepalive::new().with_time(Duration::from_secs(10)).with_interval(Duration::from_secs(5)).with_retries(3);

pub(super) enum Failure {
    /// It answered with a key other than the pinned one.
    NotPinned,
    Io(io::Error),
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Failure::Io(error)
    }
}

/// Connects, and finishes the handshake before handing over the stream, so a key that isn't pinned is refused
/// before anything is sent.
pub(super) fn connect(address: SocketAddr, fingerprint: &str, connect_timeout: Duration) -> Result<Transport, Failure> {
    let tcp = open_tcp(address, connect_timeout)?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let pinned = Arc::new(PinnedKey::new(fingerprint, provider.clone()));
    let config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(io::Error::other)?
        .dangerous()
        .with_custom_certificate_verifier(pinned.clone())
        .with_no_client_auth();
    // Nothing checks the name, since the key is pinned, but TLS needs one.
    let name = ServerName::try_from("roundhouse").map_err(io::Error::other)?;
    let mut tls = ClientConnection::new(Arc::new(config), name).map_err(io::Error::other)?;
    tcp.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    tcp.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;
    while tls.is_handshaking() {
        if let Err(e) = tls.complete_io(&mut &tcp) {
            return Err(if pinned.refused() { Failure::NotPinned } else { Failure::Io(e) });
        }
    }
    // A quiet hub sends nothing for as long as nothing changes.
    tcp.set_read_timeout(None)?;
    tcp.set_write_timeout(None)?;
    let shared = Arc::new(Shared { tls: Mutex::new(tls), tcp: tcp.try_clone()? });
    Ok(Transport {
        reader: Box::new(TlsReader { shared: shared.clone(), raw: vec![0; READ_BUFFER_BYTES], start: 0, end: 0 }),
        writer: Box::new(TlsWriter { shared }),
        closer: Closer::tcp(tcp),
    })
}

fn open_tcp(address: SocketAddr, connect_timeout: Duration) -> io::Result<TcpStream> {
    let tcp = TcpStream::connect_timeout(&address, connect_timeout)?;
    tcp.set_nodelay(true)?;
    SockRef::from(&tcp).set_tcp_keepalive(&KEEPALIVE)?;
    Ok(tcp)
}

struct Shared {
    tls: Mutex<ClientConnection>,
    tcp: TcpStream,
}

impl Shared {
    /// Sends whatever rustls has ready. Called under the lock, so records go out in the order they were made.
    fn send_ready(&self, tls: &mut ClientConnection) -> io::Result<()> {
        while tls.wants_write() {
            tls.write_tls(&mut &self.tcp)?;
        }
        Ok(())
    }
}

struct TlsReader {
    shared: Arc<Shared>,
    /// What came off the socket and hasn't been given to rustls yet.
    raw: Vec<u8>,
    start: usize,
    end: usize,
}

impl Read for TlsReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            {
                let mut tls = self.shared.tls.lock();
                match tls.reader().read(buf) {
                    // 0 is the hub saying it's finished.
                    Ok(n) => return Ok(n),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e),
                }
                if self.start < self.end {
                    // Only once what was decrypted has all been read, so rustls always has room for more.
                    self.start += tls.read_tls(&mut &self.raw[self.start..self.end])?;
                    tls.process_new_packets().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                    self.shared.send_ready(&mut tls)?;
                    continue;
                }
            }
            // Waits without the lock, so writers carry on meanwhile.
            let n = (&self.shared.tcp).read(&mut self.raw)?;
            if n == 0 {
                return Ok(0);
            }
            (self.start, self.end) = (0, n);
        }
    }
}

struct TlsWriter {
    shared: Arc<Shared>,
}

impl Write for TlsWriter {
    /// Sends what it takes before returning, so a frame never waits in rustls for the next one.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut tls = self.shared.tls.lock();
        let written = tls.writer().write(buf)?;
        self.shared.send_ready(&mut tls)?;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut tls = self.shared.tls.lock();
        self.shared.send_ready(&mut tls)
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::*;

    /// What the OS holds for the socket. A hub that's really gone can't be shown without dropping packets, which
    /// takes root, and then a test would wait the 25 s.
    #[test]
    fn a_connection_to_a_hub_probes_it_while_nothing_is_sent() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();

        let tcp = open_tcp(listener.local_addr().unwrap(), Duration::from_secs(3)).unwrap();

        let socket = SockRef::from(&tcp);
        assert!(socket.keepalive().unwrap());
        assert_eq!(socket.tcp_keepalive_time().unwrap(), Duration::from_secs(10));
        assert_eq!(socket.tcp_keepalive_interval().unwrap(), Duration::from_secs(5));
        assert_eq!(socket.tcp_keepalive_retries().unwrap(), 3);
    }
}
