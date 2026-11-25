//! What a client says of itself in its hello, and saying it.

use std::path::Path;
use std::time::Instant;

use protocol::client::{ApiError, CLIENT_PROTOCOL_VERSION, Hello, HelloReply, HubRefusing};
use protocol::model::ClientDevice;
use protocol::versions::Versions;

use super::{Client, Result};

/// What a client says of itself as it connects: what it is, such as `desktop`, and which device it runs on, if it has
/// one of its own. A name alone, such as the command line's or a test's, says no device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Introduction {
    pub client: String,
    pub device: Option<ClientDevice>,
}

impl Introduction {
    /// A client called `client`, on `device`.
    pub fn on(client: impl Into<String>, device: ClientDevice) -> Self {
        Self { client: client.into(), device: Some(device) }
    }
}

impl From<&str> for Introduction {
    fn from(client: &str) -> Self {
        Self { client: client.to_string(), device: None }
    }
}

impl Client {
    /// Connects, saying in its hello what it is and which device it's on, and checks the protocol version.
    pub fn connect(socket: &Path, introduction: impl Into<Introduction>) -> Result<Client> {
        let client = Client::open(socket)?;
        client.hello(introduction.into())?;
        Ok(client)
    }

    pub(super) fn hello(&self, introduction: Introduction) -> Result<()> {
        self.hello_until(introduction, None)
    }

    pub(super) fn hello_until(&self, introduction: Introduction, deadline: Option<Instant>) -> Result<()> {
        let Introduction { client, device } = introduction;
        // Over the network, the hub takes it to be the device it paired as, which is set already.
        if let Some(device) = &device {
            let _ = self.inner.device.set(device.id.clone());
        }
        let HelloReply { protocol_version, hub_version } = self.call_until(
            Hello {
                protocol_version: CLIENT_PROTOCOL_VERSION,
                client,
                // A client speaks only its own version, and a hub the one before its own as well after a break.
                oldest_protocol_version: Some(CLIENT_PROTOCOL_VERSION),
                device,
            },
            deadline,
        )?;
        // A client speaks only its own version, so it can't follow a hub that chose another.
        if protocol_version != CLIENT_PROTOCOL_VERSION {
            let message =
                format!("the hub chose protocol {protocol_version}, and this client speaks {CLIENT_PROTOCOL_VERSION}");
            let hub = HubRefusing { version: hub_version, speaks: Versions::only(protocol_version) };
            return Err(ApiError::versions_differ(message, hub).into());
        }
        let _ = self.inner.hub_version.set(hub_version);
        Ok(())
    }
}
