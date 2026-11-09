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
use super::serve_connection;
use crate::hub::Hub;

/// A phone can go without closing its connection, such as when it leaves the network or its battery runs out.
/// Nothing is sent on a quiet connection, so nothing would notice, and it would count as connected for good, with
/// what it subscribed to still read for it. Probing after 10 s of quiet, every 5 s, and giving up after 3 unanswered
/// probes closes it within about 25 s.
const KEEPALIVE: TcpKeepalive =
    TcpKeepalive::new().with_time(Duration::from_secs(10)).with_interval(Duration::from_secs(5)).with_retries(3);

/// Boxed, because pairing starts this, and a request to pair is served by it. Its type would otherwise be part of
/// its own definition.
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
                        debug!(%peer, "closed a connection, because too many haven't authenticated yet");
                        continue;
                    };
                    tokio::spawn(serve_device(hub.clone(), tls.clone(), stream, peer, deadline, unproven));
                }
                Err(e) => {
                    warn!("accepting a connection from the network failed: {e}");
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
        debug!(%peer, "setting up a connection from the network failed: {e}");
        return;
    }
    match tokio::time::timeout_at(deadline, tls.accept(stream)).await {
        Ok(Ok(stream)) => serve_connection(hub, stream, Gate::network(deadline, unproven)).await,
        Ok(Err(e)) => debug!(%peer, "a TLS handshake failed: {e}"),
        Err(_) => debug!(%peer, "a TLS handshake took too long"),
    }
}

fn ready(stream: &TcpStream) -> io::Result<()> {
    stream.set_nodelay(true)?;
    SockRef::from(stream).set_tcp_keepalive(&KEEPALIVE)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(socket.tcp_keepalive_time().unwrap(), Duration::from_secs(10));
        assert_eq!(socket.tcp_keepalive_interval().unwrap(), Duration::from_secs(5));
        assert_eq!(socket.tcp_keepalive_retries().unwrap(), 3);
    }
}
