//! Choosing which devices can reach the hub over the network. Only a client on the hub's own machine can, and a
//! hub older than these refuses them, as `is_unsupported` says.

use protocol::client::{ClientRequest, ClientResponse};
use protocol::model::DeviceId;
use protocol::pairing::{Device, Pairing, PairingCode};

use super::{Client, Result, unexpected};

impl Client {
    /// A new pairing code, in a link for a phone to open. The hub listens on the network from now on, until no
    /// code is live and no device is paired.
    pub fn pairing_start(&self) -> Result<Pairing> {
        let reply = self.call(ClientRequest::PairingStart)?;
        let ClientResponse::Pairing(pairing) = reply else {
            return unexpected(&reply);
        };
        Ok(pairing)
    }

    /// Waits until a device pairs with this code, and returns it. Fails with `NotFound` once the code expires.
    pub fn pairing_wait(&self, code: &PairingCode) -> Result<Device> {
        let reply = self.call(ClientRequest::PairingWait { code: code.clone() })?;
        let ClientResponse::Device(device) = reply else {
            return unexpected(&reply);
        };
        Ok(device)
    }

    pub fn device_list(&self) -> Result<Vec<Device>> {
        let reply = self.call(ClientRequest::DeviceList)?;
        let ClientResponse::Devices { devices } = reply else {
            return unexpected(&reply);
        };
        Ok(devices)
    }

    /// Forgets the device and closes its connections, so it has to pair again.
    pub fn device_revoke(&self, id: &DeviceId) -> Result<()> {
        let reply = self.call(ClientRequest::DeviceRevoke { id: id.clone() })?;
        let ClientResponse::Ok = reply else {
            return unexpected(&reply);
        };
        Ok(())
    }
}
