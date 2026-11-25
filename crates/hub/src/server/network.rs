//! Accepting connections from devices. Anyone who can reach the port can open one, so what a connection can cost
//! the hub before it authenticates is bounded: a few at a time, each for a few seconds.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use socket2::{SockRef, TcpKeepalive};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::OwnedSemaphorePermit;
use tokio::time::Instant;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, warn};

use super::admit::{AUTHENTICATE_WITHIN, Gate};
use super::connection::serve_connection;
use super::unsent::set_most_unsent;
use crate::hub::Hub;

/// A phone can go without closing its connection, such as when it leaves the network or its battery runs out.
/// Nothing is sent on a quiet connection, so nothing would notice, and it would count as connected for good, with
/// what it subscribed to still read for it. Probing after 10 s of quiet, every 5 s, and giving up after 3 unanswered
/// probes closes it within about 25 s.
const KEEPALIVE: TcpKeepalive =
    TcpKeepalive::new().with_time(Duration::from_secs(10)).with_interval(Duration::from_secs(5)).with_retries(3);

/// The most of what's written to a device's connection that the kernel holds unsent. The hub keeps one frame of a
/// watched terminal queued for a client at a time, and lets the next go once that one is written, which means taken by
/// the kernel, not sent. Left to size its send buffer by itself, the kernel takes frame after frame while a slow link
/// sends them, and a reply written after them waits for them all: at 1 Mbit/s, under a flood, 1.4 s, against 0.45 s
/// with this, as #2129 measured. 16 KiB is about a tenth of a second at 1 Mbit/s, and enough that the kernel has
/// something to send while the hub writes the next frame.
const MOST_UNSENT: u32 = 16 * 1024;

/// Boxed, because pairing starts this, and a request to pair is served by it. Its type would otherwise be part of
/// its own definition.
#[expect(
    clippy::disallowed_methods,
    reason = "the pause after a connection that couldn't be accepted, for what it needed to come free"
)]
pub(crate) fn accept_devices(
    hub: Arc<Hub>,
    listener: TcpListener,
    tls: TlsAcceptor,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    Box::pin(async move {
        loop {
            match listener.accept().await {
                Ok((stream, peer)) => {
                    let deadline = Instant::now() + AUTHENTICATE_WITHIN;
                    // Taken before the handshake, which is most of what a connection costs before it authenticates.
                    // One that finds no room is closed at once, so connections that never authenticate can't use up
                    // the descriptors the hub's socket and workers need too.
                    let Ok(unproven) = hub.remote.unauthenticated.clone().try_acquire_owned() else {
                        debug!(
                            name: "client.too_many_unauthenticated",
                            %peer,
                            "Closed a connection, because too many haven't authenticated yet"
                        );
                        continue;
                    };
                    tokio::spawn(serve_device(hub.clone(), tls.clone(), stream, peer, deadline, unproven));
                }
                Err(e) => {
                    warn!(
                        name: "client.network_connection_failed",
                        error = %e,
                        "Couldn't accept a connection from the network"
                    );
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    })
}

async fn serve_device(
    hub: Arc<Hub>,
    tls: TlsAcceptor,
    stream: TcpStream,
    peer: SocketAddr,
    deadline: Instant,
    unproven: OwnedSemaphorePermit,
) {
    if let Err(e) = ready(&stream) {
        debug!(
            name: "client.network_connection_unready",
            %peer,
            error = %e,
            "Couldn't set up a connection from the network"
        );
        return;
    }
    match tokio::time::timeout_at(deadline, tls.accept(stream)).await {
        Ok(Ok(stream)) => {
            let (read, write) = tokio::io::split(stream);
            serve_connection(hub, read, write, Gate::network(deadline, unproven)).await;
        }
        Ok(Err(e)) => debug!(name: "client.tls_failed", %peer, error = %e, "A TLS handshake failed"),
        Err(_) => debug!(name: "client.tls_late", %peer, "A TLS handshake took too long"),
    }
}

fn ready(stream: &TcpStream) -> io::Result<()> {
    stream.set_nodelay(true)?;
    set_most_unsent(stream, MOST_UNSENT)?;
    SockRef::from(stream).set_tcp_keepalive(&KEEPALIVE)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::server::unsent::most_unsent;

    /// What the OS holds for the socket. A phone that's really gone can't be shown without dropping packets, which
    /// takes root, and then a test would wait the 25 s.
    #[tokio::test]
    async fn a_device_is_probed_while_its_connection_is_quiet() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let _device = TcpStream::connect(listener.local_addr().unwrap()).await.unwrap();
        let (accepted, _) = listener.accept().await.unwrap();

        ready(&accepted).unwrap();

        let socket = SockRef::from(&accepted);
        assert!(socket.keepalive().unwrap());
        // socket2 cannot read the keepalive time back on Windows.
        #[cfg(unix)]
        assert_eq!(socket.tcp_keepalive_time().unwrap(), Duration::from_secs(10));
        // socket2 cannot read the interval back on Windows, although ready sets it there too.
        #[cfg(unix)]
        assert_eq!(socket.tcp_keepalive_interval().unwrap(), Duration::from_secs(5));
        assert_eq!(socket.tcp_keepalive_retries().unwrap(), 3);
    }

    /// So a reply to a device on a slow link waits for little more than the frame being written, as `MOST_UNSENT` says.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_devices_connection_holds_little_unsent() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let _device = TcpStream::connect(listener.local_addr().unwrap()).await.unwrap();
        let (accepted, _) = listener.accept().await.unwrap();

        ready(&accepted).unwrap();

        assert_eq!(most_unsent(&accepted).unwrap(), MOST_UNSENT);
    }
}
