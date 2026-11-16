//! Requests that only a client on the hub's machine may make.
use super::{Client, Result};
use protocol::client::devices;
use protocol::model::DeviceId;
use protocol::pairing::{Device, Pairing, PairingCode};
impl Client {
    pub fn pairing_start(&self) -> Result<Pairing> {
        self.call(devices::PairingStart)
    }
    pub fn pairing_wait(&self, code: &PairingCode) -> Result<Device> {
        self.call(devices::PairingWait { code: code.clone() })
    }
    pub fn device_list(&self) -> Result<Vec<Device>> {
        Ok(self.call(devices::DeviceList)?.devices)
    }
    pub fn device_revoke(&self, id: &DeviceId) -> Result<()> {
        self.call(devices::DeviceRevoke { id: id.clone() })
    }
}
