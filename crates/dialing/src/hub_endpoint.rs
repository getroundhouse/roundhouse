use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::time::Duration;

use iroh::endpoint::PortmapperConfig;
use iroh::endpoint::presets;
use iroh::{Endpoint, RelayMode};
use protocol::dialing::{HubAddress, WORKER_ALPN};
use tracing::{info, warn};

use crate::{DialError, key_file};

/// How long a ticket waits for the hub's endpoint to reach its relay, so the ticket names it. A hub whose relay is out
/// of reach still makes one, with its direct addresses alone, so a worker on its network still reaches it.
const ONLINE_WITHIN: Duration = Duration::from_secs(5);

/// How long the hub tries to bind the port its endpoint had before it takes another, which a worker that dials it
/// again then reaches only through the relay, or, on loopback, not at all.
const BOUND_AGAIN_WITHIN: Duration = Duration::from_secs(2);

/// The UDP ports a loopback endpoint binds, below the range any of the systems the hub runs on gives out for ephemeral
/// ports: Linux's starts at 32768, and macOS's and Windows' at 49152. So no program's socket bound to any port takes
/// one while the hub restarts, and a worker on the hub's machine, with no relay to fall back on, finds the hub where
/// its ticket says. Above the ports a system keeps for its own services.
pub(crate) const LOOPBACK_PORTS: std::ops::Range<u16> = 20_000..32_768;

/// How many ports of [`LOOPBACK_PORTS`] a loopback endpoint tries before it gives up.
const LOOPBACK_TRIES: u16 = 64;

/// Which relays the hub's endpoint uses, which the tickets it makes name, so its workers use them too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relays {
    /// iroh's default, n0's public relays, until a hub can name its own.
    N0,
    /// None, so every connection is direct, and the endpoint binds only to loopback, at a port in
    /// [`LOOPBACK_PORTS`], which keeps it off the internet. The endpoint the hub's own machine's workers dial is
    /// this, and a test's other endpoint too.
    None,
}

/// The hub's end: one endpoint, which every worker dials, by the key kept in the data directory.
pub struct HubEndpoint {
    endpoint: Endpoint,
    relays: Relays,
}

impl HubEndpoint {
    /// Binds the endpoint with the key at `key_file`, made first if there's none, on the UDP port it bound last, or on
    /// another if that one's taken, which it keeps for next time.
    pub async fn bind(key_file: &Path, relays: Relays) -> Result<Self, DialError> {
        let (key, port) = key_file::read_or_make(key_file)?;
        let endpoint = match port {
            Some(kept) => match bind_again(&key, relays, kept).await {
                Ok(endpoint) => endpoint,
                Err(e) => {
                    warn!(name: "endpoint.port_taken", port = kept, error = %e, "Couldn't bind the port the hub's endpoint had, so it takes another");
                    bind_another(&key, relays).await?
                }
            },
            None => bind_another(&key, relays).await?,
        };
        let bound = endpoint.bound_sockets().iter().find(|socket| socket.is_ipv4()).map(SocketAddr::port);
        if bound != port {
            key_file::write(key_file, &key, bound)?;
        }
        info!(name: "endpoint.bound", key = %endpoint.id(), port = ?bound, "Bound the endpoint workers dial");
        Ok(Self { endpoint, relays })
    }

    /// Where it is now, for a ticket, once it has reached its relay or given up waiting to.
    pub async fn address(&self) -> HubAddress {
        if self.relays == Relays::N0 && tokio::time::timeout(ONLINE_WITHIN, self.endpoint.online()).await.is_err() {
            warn!(name: "endpoint.offline", "The hub's endpoint hasn't reached its relay, so a ticket names its direct addresses alone");
        }
        let addr = self.endpoint.addr();
        HubAddress {
            key: addr.id.to_string(),
            relay: addr.relay_urls().next().map(ToString::to_string),
            direct: addr.ip_addrs().copied().collect(),
        }
    }

    /// The next connection a worker opens, or `None` once the endpoint is closed.
    pub async fn accept(&self) -> Option<iroh::endpoint::Incoming> {
        self.endpoint.accept().await
    }

    /// Closes every connection, telling each worker so.
    pub async fn close(&self) {
        self.endpoint.close().await;
    }
}

/// Binds `port` again, trying for [`BOUND_AGAIN_WITHIN`], since the endpoint that had it may not have let it go yet,
/// as when a hub restarts at once.
#[expect(clippy::disallowed_methods, reason = "the pace between tries to bind a port that's still being let go of")]
async fn bind_again(key: &iroh::SecretKey, relays: Relays, port: u16) -> Result<Endpoint, DialError> {
    let deadline = tokio::time::Instant::now() + BOUND_AGAIN_WITHIN;
    loop {
        match bind(key, relays, port).await {
            Ok(endpoint) => return Ok(endpoint),
            Err(e) if tokio::time::Instant::now() >= deadline => return Err(e),
            Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
        }
    }
}

/// Binds a port of its own choosing: any the system gives, or, on loopback, the first of [`LOOPBACK_PORTS`] that's
/// free, from one the key picks, so hubs with different keys mostly try different ones.
async fn bind_another(key: &iroh::SecretKey, relays: Relays) -> Result<Endpoint, DialError> {
    if relays == Relays::N0 {
        return bind(key, relays, 0).await;
    }
    let ports = LOOPBACK_PORTS.end - LOOPBACK_PORTS.start;
    let picked = key
        .public()
        .as_bytes()
        .iter()
        .fold(0u16, |picked, byte| picked.wrapping_mul(31).wrapping_add(u16::from(*byte)));
    let mut last = None;
    for tried in 0..LOOPBACK_TRIES {
        let port = LOOPBACK_PORTS.start + picked.wrapping_add(tried) % ports;
        match bind(key, relays, port).await {
            Ok(endpoint) => return Ok(endpoint),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| DialError::Bind("no port to try".into())))
}

async fn bind(key: &iroh::SecretKey, relays: Relays, port: u16) -> Result<Endpoint, DialError> {
    let builder = Endpoint::builder(presets::Minimal).secret_key(key.clone()).alpns(vec![WORKER_ALPN.to_vec()]);
    let builder = match relays {
        Relays::N0 => builder.relay_mode(RelayMode::Default).bind_addr(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))),
        Relays::None => builder
            .relay_mode(RelayMode::Disabled)
            .portmapper_config(PortmapperConfig::Disabled)
            .clear_ip_transports()
            .bind_addr(SocketAddr::from((Ipv4Addr::LOCALHOST, port))),
    };
    let builder = builder.map_err(|e| DialError::Bind(e.to_string()))?;
    builder.bind().await.map_err(|e| DialError::Bind(e.to_string()))
}
