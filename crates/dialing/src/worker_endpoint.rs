use std::net::{Ipv4Addr, SocketAddr};

use iroh::endpoint::PortmapperConfig;
use iroh::endpoint::{Connection, RecvStream, SendStream, presets};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode, RelayUrl};
use protocol::dialing::{HubAddress, TicketSecret, WORKER_ALPN};
use protocol::model::InstanceId;

use crate::DialError;

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
    endpoint: Endpoint,
    hub: EndpointAddr,
}

impl WorkerEndpoint {
    /// Binds the UDP `port`, or any if it's `None`, and advertises `external`, if it's given, as an address the hub can
    /// reach it at directly, such as the host's end of a port forwarded into a VM. It uses the relay the hub does, or
    /// none if the ticket names none.
    pub async fn bind(hub: &HubAddress, port: Option<u16>, external: Option<SocketAddr>) -> Result<Self, DialError> {
        let ticket = |e: &dyn std::fmt::Display| DialError::Ticket(e.to_string());
        let id: EndpointId = hub.key.parse().map_err(|e| ticket(&e))?;
        let mut addr = EndpointAddr::new(id);
        for direct in &hub.direct {
            addr = addr.with_ip_addr(*direct);
        }
        let relay = hub.relay.as_deref().map(str::parse::<RelayUrl>).transpose().map_err(|e| ticket(&e))?;
        let builder = Endpoint::builder(presets::Minimal);
        let builder = match relay {
            Some(relay) => {
                addr = addr.with_relay_url(relay.clone());
                builder.relay_mode(RelayMode::Custom(relay.into()))
            }
            None => builder.relay_mode(RelayMode::Disabled).portmapper_config(PortmapperConfig::Disabled),
        };
        let mut builder = builder
            .bind_addr(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port.unwrap_or(0))))
            .map_err(|e| DialError::Bind(e.to_string()))?;
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
