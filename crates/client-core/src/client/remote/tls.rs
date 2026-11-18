//! TLS over TCP, blocking. rustls keeps a connection's state in one object, and a client reads on one thread while
//! writing from others. The reader waits outside the TLS lock; writes under it have one deadline for the entire
//! frame. Any failed write shuts down the socket, releasing the reader and every pending request.

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use protocol::pairing::Fingerprint;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection};
use socket2::{SockRef, TcpKeepalive};

use super::pin::PinnedKey;
use crate::client::transport::{Closer, Transport};

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
/// before anything is sent. Connecting and the whole handshake have until `deadline`, so something that accepts the
/// connection but never answers, or answers a byte at a time, can't hold it longer.
pub(super) fn connect(
    address: SocketAddr,
    fingerprint: &Fingerprint,
    deadline: Instant,
    write_timeout: Duration,
) -> Result<Transport, Failure> {
    let tcp = open_tcp(address, time_left(deadline)?)?;
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
    while tls.is_handshaking() {
        if let Err(e) = tls.complete_io(&mut Until { tcp: &tcp, deadline }) {
            return Err(if pinned.refused() { Failure::NotPinned } else { Failure::Io(e) });
        }
    }
    // A quiet hub sends nothing for as long as nothing changes.
    tcp.set_read_timeout(None)?;
    let shared = Arc::new(Shared { tls: Mutex::new(tls), tcp: tcp.try_clone()?, write_timeout });
    Ok(Transport {
        reader: Box::new(TlsReader { shared: shared.clone(), raw: vec![0; READ_BUFFER_BYTES], start: 0, end: 0 }),
        writer: Box::new(TlsWriter { shared }),
        closer: Closer::tcp(tcp),
    })
}

/// What's left before `deadline`, or an error once there's nothing.
fn time_left(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "the hub did not finish the network operation in time"))
}

/// The socket, with each read and write given only what's left before `deadline`. rustls reads as often as it needs
/// to in one call, so a timeout set once would apply to each read rather than to the whole handshake.
struct Until<'a> {
    tcp: &'a TcpStream,
    deadline: Instant,
}

impl Read for Until<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.tcp.set_read_timeout(Some(time_left(self.deadline)?))?;
        timed_out_if_blocked(self.tcp.read(buf))
    }
}

impl Write for Until<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let left = time_left(self.deadline)?;
        self.tcp.set_write_timeout(Some(left))?;
        let t = Instant::now();
        let r = timed_out_if_blocked(self.tcp.write(buf));
        if t.elapsed() > left + Duration::from_millis(100) {
            eprintln!("PROBE tcp write of {} with {left:?} left took {:?}: {r:?}", buf.len(), t.elapsed());
        }
        r
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A socket's timeout comes back as `WouldBlock` on Unix, which reads as though nothing was wrong.
fn timed_out_if_blocked<T>(result: io::Result<T>) -> io::Result<T> {
    result.map_err(|e| {
        if e.kind() == io::ErrorKind::WouldBlock {
            io::Error::new(io::ErrorKind::TimedOut, "the hub didn't finish connecting in time")
        } else {
            e
        }
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
    /// Even a live peer can stop reading. A frame that cannot finish is ambiguous, so its connection is discarded.
    write_timeout: Duration,
}

impl Shared {
    /// Sends whatever rustls has ready. Called under the lock, so records go out in the order they were made.
    fn send_ready(&self, tls: &mut ClientConnection, deadline: Instant) -> io::Result<()> {
        let result = self.write_ready(tls, deadline);
        if result.is_err() {
            let _ = self.tcp.shutdown(Shutdown::Both);
        }
        result
    }

    fn write_ready(&self, tls: &mut ClientConnection, deadline: Instant) -> io::Result<()> {
        while tls.wants_write() {
            if tls.write_tls(&mut Until { tcp: &self.tcp, deadline })? == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
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
                    self.shared.send_ready(&mut tls, Instant::now() + self.shared.write_timeout)?;
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
        self.shared.send_ready(&mut tls, Instant::now() + self.shared.write_timeout)?;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut tls = self.shared.tls.lock();
        self.shared.send_ready(&mut tls, Instant::now() + self.shared.write_timeout)
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        let start = Instant::now();
        let len = buf.len();
        let result = self.write_all_probe(buf);
        if len > 1 << 20 || start.elapsed() > Duration::from_millis(300) {
            eprintln!("PROBE write_all of {len} took {:?}: {result:?}", start.elapsed());
        }
        result
    }
}

impl TlsWriter {
    fn write_all_probe(&mut self, mut buf: &[u8]) -> io::Result<()> {
        let locking = Instant::now();
        let deadline = Instant::now() + self.shared.write_timeout;
        let mut tls = self.shared.tls.lock();
        if locking.elapsed() > Duration::from_millis(50) {
            eprintln!("PROBE tls lock took {:?}", locking.elapsed());
        }
        while !buf.is_empty() {
            let t = Instant::now();
            let written = tls.writer().write(buf)?;
            let encrypted = t.elapsed();
            let r = self.shared.send_ready(&mut tls, deadline);
            if t.elapsed() > Duration::from_millis(250) || r.is_err() {
                eprintln!("PROBE chunk {written}: encrypt {encrypted:?}, send {:?} {r:?}, left {}", t.elapsed(), buf.len());
            }
            r?;
            if written == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            buf = &buf[written..];
        }
        Ok(())
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
        keepalive_timing(&socket);
        assert_eq!(socket.tcp_keepalive_retries().unwrap(), 3);
    }

    // Windows sets these intervals through WSAIoctl but socket2 exposes their getters only on Unix.
    #[cfg(unix)]
    fn keepalive_timing(socket: &SockRef<'_>) {
        assert_eq!(socket.tcp_keepalive_time().unwrap(), Duration::from_secs(10));
        assert_eq!(socket.tcp_keepalive_interval().unwrap(), Duration::from_secs(5));
    }
    #[cfg(windows)]
    fn keepalive_timing(_: &SockRef<'_>) {}

    /// Each read waiting its own time would let this one go on for as long as the bytes kept coming, which is far
    /// longer than the test waits for it to give up, so one that gives up within that gave up at its deadline.
    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "the pace of a host that answers a byte at a time, which keeps each read short of its own time limit"
    )]
    fn a_host_that_answers_a_byte_at_a_time_is_given_up_on_at_the_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            // The start of a TLS record of 16 KiB, the most one holds, then the record a byte at a time, which at
            // this pace takes over ten minutes.
            let mut next: &[u8] = &[0x16, 0x03, 0x03, 0x40, 0x00];
            while stream.write_all(next).is_ok() {
                std::thread::sleep(Duration::from_millis(50));
                next = &[0];
            }
        });
        let (gave_up, given_up) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_millis(500);
            let failed = connect(address, &Fingerprint::of_sha256([0; 32]), deadline, Duration::from_secs(1));
            let timed_out = matches!(&failed, Err(Failure::Io(e)) if e.kind() == io::ErrorKind::TimedOut);
            gave_up.send(timed_out).unwrap();
        });

        let timed_out = waiting::thread::until_ok(Duration::from_secs(10), || {
            given_up.try_recv().map_err(|_| "it went on reading for as long as the bytes kept coming")
        });

        assert!(timed_out, "it failed, but not by timing out");
    }
}
