//! Reaching the hub over the network: its certificate, the TCP listener for paired devices, pairing codes, and the
//! connections each device has open, so revoking one can close them. What a request does with these is in
//! `hub::devices`.

pub(crate) mod certificate;
pub(crate) mod codes;
pub(crate) mod machine;

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::model::DeviceId;
use protocol::pairing::Fingerprint;
use protocol::paths::ENV_PORT;
use ring::rand::SecureRandom;
use tokio::sync::{Mutex as AsyncMutex, Notify, Semaphore, watch};
use tokio::task::JoinHandle;
use tracing::info;

use crate::error::internal;
use crate::hub::Hub;
use certificate::Identity;
use codes::Codes;

/// How long a pairing code works.
const PAIRING_LIFE: Duration = Duration::from_mins(10);

/// How many connections from the network can be waiting to authenticate at once. A phone authenticates within a
/// second of connecting, so this leaves room for several at once, and one more is closed straight away.
pub const MAX_UNAUTHENTICATED: usize = 16;

pub(crate) struct RemoteConfig {
    /// 0 lets the OS choose.
    pub(crate) port: u16,
    pub(crate) certificate: PathBuf,
    pub(crate) certificate_key: PathBuf,
}

pub(crate) struct Remote {
    config: RemoteConfig,
    ip: IpAddr,
    pub(crate) pairing_life: Duration,
    /// Loaded or made the first time the hub listens, and kept.
    identity: Mutex<Option<Arc<Identity>>>,
    /// Held while devices, codes or the listener change, and while a device authenticates, so each sees the
    /// others' changes whole: a revoke can't miss a connection that's authenticating, and the listener can't stop
    /// while a device is being saved.
    pub(crate) changing: AsyncMutex<()>,
    listening: Mutex<Option<Listening>>,
    pub(crate) codes: Mutex<Codes>,
    /// Woken when a device pairs.
    pub(crate) paired: Notify,
    /// Each device's open connections, and how to close each one.
    connections: Mutex<HashMap<DeviceId, HashMap<u64, Revoke>>>,
    next_connection: AtomicU64,
    /// A place for each connection from the network that hasn't authenticated yet, up to [`MAX_UNAUTHENTICATED`].
    pub(crate) unauthenticated: Arc<Semaphore>,
}

struct Listening {
    port: u16,
    accept: JoinHandle<()>,
}

/// Set when a connection's device is revoked. The connection holds it from when it's accepted, and watches it around
/// everything it does, so revoking closes it however it's waiting, such as on a device that has stopped reading.
pub(crate) type Revoke = Arc<watch::Sender<bool>>;

/// One connection's claim to be a device.
pub(crate) struct DeviceLink {
    pub(crate) device_id: DeviceId,
    connection: u64,
}

impl Remote {
    pub(crate) fn new(config: RemoteConfig) -> Remote {
        Remote {
            config,
            ip: listen_ip(),
            pairing_life: pairing_life(),
            identity: Mutex::new(None),
            changing: AsyncMutex::new(()),
            listening: Mutex::new(None),
            codes: Mutex::new(Codes::default()),
            paired: Notify::new(),
            connections: Mutex::new(HashMap::new()),
            next_connection: AtomicU64::new(1),
            unauthenticated: Arc::new(Semaphore::new(MAX_UNAUTHENTICATED)),
        }
    }

    /// The port it's listening on, while it is.
    pub(crate) fn port(&self) -> Option<u16> {
        self.listening.lock().as_ref().map(|listening| listening.port)
    }

    /// Starts listening, if it isn't, and returns the port. Making the certificate the first time blocks, so it
    /// runs aside.
    pub(crate) async fn listen(&self, hub: &Arc<Hub>) -> Result<u16, ApiError> {
        if let Some(port) = self.port() {
            return Ok(port);
        }
        let identity = self.identity().await?;
        let address = SocketAddr::new(self.ip, self.config.port);
        let listener = tokio::net::TcpListener::bind(address).await.map_err(|e| {
            let message =
                format!("couldn't listen for devices on port {}: {e}. {ENV_PORT} chooses another", address.port());
            ApiError::new(ErrorCode::Conflict, message)
        })?;
        let port = listener.local_addr().map_err(|e| internal(format!("reading the port listened on: {e}")))?.port();
        let accept = tokio::spawn(crate::server::accept_devices(hub.clone(), listener, identity.acceptor.clone()));
        *self.listening.lock() = Some(Listening { port, accept });
        info!(port, "listening for paired devices");
        Ok(port)
    }

    /// Connections already open carry on, but nothing new can connect. Returns once the port is free, so listening
    /// again straight away can have it.
    pub(crate) async fn stop_listening(&self) {
        let listening = self.listening.lock().take();
        let Some(listening) = listening else { return };
        listening.accept.abort();
        // Finishes once the task, and the listener it owns, have been dropped.
        let _ = listening.accept.await;
        info!(port = listening.port, "stopped listening for devices");
    }

    pub(crate) async fn fingerprint(&self) -> Result<Fingerprint, ApiError> {
        Ok(self.identity().await?.fingerprint.clone())
    }

    async fn identity(&self) -> Result<Arc<Identity>, ApiError> {
        if let Some(identity) = self.identity.lock().clone() {
            return Ok(identity);
        }
        let (certificate, key) = (self.config.certificate.clone(), self.config.certificate_key.clone());
        let identity = tokio::task::spawn_blocking(move || certificate::load_or_create(&certificate, &key))
            .await
            .map_err(|e| internal(e.to_string()))?
            .map_err(|e| internal(format!("the hub's certificate: {}", error_chain(&e))))?;
        Ok(self.identity.lock().get_or_insert(Arc::new(identity)).clone())
    }

    /// Notes a connection as this device's, so revoking the device sets the connection's `revoke`.
    pub(crate) fn connected(&self, device_id: &DeviceId, revoke: &Revoke) -> DeviceLink {
        let connection = self.next_connection.fetch_add(1, Ordering::Relaxed);
        self.connections.lock().entry(device_id.clone()).or_default().insert(connection, revoke.clone());
        DeviceLink { device_id: device_id.clone(), connection }
    }

    pub(crate) fn disconnected(&self, link: &DeviceLink) {
        let mut connections = self.connections.lock();
        if let Some(open) = connections.get_mut(&link.device_id) {
            open.remove(&link.connection);
            if open.is_empty() {
                connections.remove(&link.device_id);
            }
        }
    }

    /// Closes every connection this device has open.
    pub(crate) fn close_all(&self, device_id: &DeviceId) {
        for (_, revoke) in self.connections.lock().remove(device_id).unwrap_or_default() {
            revoke.send_replace(true);
        }
    }

    pub(crate) fn connection_count(&self, device_id: &DeviceId) -> u32 {
        self.connections.lock().get(device_id).map_or(0, |open| u32::try_from(open.len()).unwrap_or(u32::MAX))
    }
}

/// `N` random bytes, in unpadded base64url.
pub(crate) fn random_text<const N: usize>() -> Result<String, ApiError> {
    let mut bytes = [0u8; N];
    ring::rand::SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| internal("the system's random number generator failed"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// [`PAIRING_LIFE`], unless a test has set `ENV_PAIRING_LIFE_MS`, which only a `testing` build reads.
fn pairing_life() -> Duration {
    #[cfg(feature = "testing")]
    if let Some(ms) = std::env::var(protocol::paths::ENV_PAIRING_LIFE_MS).ok().and_then(|ms| ms.parse().ok()) {
        return Duration::from_millis(ms);
    }
    PAIRING_LIFE
}

/// Every interface, unless a test has set `ENV_LISTEN_ON_LOOPBACK`, which only a `testing` build reads. A Mac's
/// firewall asks before a new binary accepts connections from the network, and tests build a new one each time.
fn listen_ip() -> IpAddr {
    #[cfg(feature = "testing")]
    if std::env::var_os(protocol::paths::ENV_LISTEN_ON_LOOPBACK).is_some() {
        return IpAddr::V4(Ipv4Addr::LOCALHOST);
    }
    IpAddr::V4(Ipv4Addr::UNSPECIFIED)
}
