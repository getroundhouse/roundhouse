use std::net::{Ipv4Addr, SocketAddr};

use std::time::Duration;

use iroh::endpoint::PortmapperConfig;
use iroh::endpoint::{Connection, IdleTimeout, QuicTransportConfig, RecvStream, SendStream, presets};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode, RelayUrl};
use protocol::dialing::{HubAddress, TicketSecret, WORKER_ALPN};
use protocol::model::InstanceId;

use crate::DialError;

/// How often a worker that dials a hub with no relay, as one on the hub's own machine does, makes sure its connection
/// is still there when nothing else crosses it.
const KEEP_ALIVE_EVERY: Duration = Duration::from_secs(1);

/// How long such a worker's connection goes without a word from the hub before the worker takes it for gone and dials
/// again. A hub that's killed says nothing, and the next one can't end what the last had, so this is how a worker finds
/// it has gone, and the next hub waits only five seconds for the workers the last one left to dial it. iroh's own 30
/// seconds is for a path across networks, which can go quiet for a while and come back.
const NO_RELAY_IDLE_WITHIN: Duration = Duration::from_secs(3);

/// A worker its hub admitted, with the stream the hub did so on.
pub struct Admitted {
    pub instance_id: InstanceId,
    /// Held as long as the streams are, since the connection closes once it's dropped.
    pub connection: Connection,
    pub send: SendStream,
    pub recv: RecvStream,
}

/// A worker's end, which dials its hub at the address its ticket names, with a key of its own that lasts as long as
/// the process does, which the hub knows it by once it has been admitted.
pub struct WorkerEndpoint {
    pub(crate) endpoint: Endpoint,
    hub: EndpointAddr,
}

impl WorkerEndpoint {
    /// Binds the UDP `port`, or any if it's `None`, and advertises `external`, if it's given, as an address the hub can
    /// reach it at directly, such as the host's end of a port forwarded into a VM. It uses the relay the hub does, or
    /// none if the ticket names none. A worker given neither, whose hub is only on loopback with no relay, as a local
    /// worker's is, binds only to loopback, as that hub does, so it's never on the network, where Windows' firewall
    /// would ask whether each build's copy of the program may take connections.
    pub async fn bind(hub: &HubAddress, port: Option<u16>, external: Option<SocketAddr>) -> Result<Self, DialError> {
        let ticket = |e: &dyn std::fmt::Display| DialError::Ticket(e.to_string());
        let id: EndpointId = hub.key.parse().map_err(|e| ticket(&e))?;
        let mut addr = EndpointAddr::new(id);
        for direct in &hub.direct {
            addr = addr.with_ip_addr(*direct);
        }
        let relay = hub.relay.as_deref().map(str::parse::<RelayUrl>).transpose().map_err(|e| ticket(&e))?;
        let on_loopback = port.is_none()
            && external.is_none()
            && relay.is_none()
            && !hub.direct.is_empty()
            && hub.direct.iter().all(|direct| direct.ip().is_loopback());
        let builder = Endpoint::builder(presets::Minimal);
        let builder = if let Some(relay) = relay {
            addr = addr.with_relay_url(relay.clone());
            builder.relay_mode(RelayMode::Custom(relay.into()))
        } else {
            let idle = IdleTimeout::try_from(NO_RELAY_IDLE_WITHIN).map_err(|e| DialError::Bind(e.to_string()))?;
            let transport =
                QuicTransportConfig::builder().keep_alive_interval(KEEP_ALIVE_EVERY).max_idle_timeout(Some(idle));
            builder
                .relay_mode(RelayMode::Disabled)
                .portmapper_config(PortmapperConfig::Disabled)
                .transport_config(transport.build())
        };
        let builder = if on_loopback {
            builder.clear_ip_transports().bind_addr(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        } else {
            builder.bind_addr(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port.unwrap_or(0))))
        };
        let mut builder = builder.map_err(|e| DialError::Bind(e.to_string()))?;
        if let Some(external) = external {
            builder = builder.external_addr(external);
        }
        let endpoint = builder.bind().await.map_err(|e| DialError::Bind(e.to_string()))?;
        Ok(Self { endpoint, hub: addr })
    }

    /// A connection to the hub, which says nothing yet.
    pub async fn dial(&self) -> Result<Connection, DialError> {
        self.endpoint.connect(self.hub.clone(), WORKER_ALPN).await.map_err(|e| DialError::Dial(e.to_string()))
    }

    /// A connection to the hub that has admitted this worker with `secret`, or with the key it admitted it by before,
    /// and the stream it did so on, which carries the worker protocol from then on.
    pub async fn dial_admitted(&self, secret: &TicketSecret) -> Result<Admitted, DialError> {
        let connection = self.dial().await?;
        let (mut send, mut recv) = connection.open_bi().await.map_err(|e| DialError::Dial(e.to_string()))?;
        let admitted = crate::admission::say(&mut send, &mut recv, secret.clone()).await?;
        Ok(Admitted { connection, send, recv, instance_id: admitted.instance_id })
    }

    /// The key the hub knows it by, as iroh writes it.
    pub fn key(&self) -> String {
        self.endpoint.id().to_string()
    }

    pub async fn close(&self) {
        self.endpoint.close().await;
    }
}
