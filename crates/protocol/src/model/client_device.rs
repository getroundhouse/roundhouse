//! Which device a client runs on, as it says in its hello.

use serde::{Deserialize, Serialize};

use super::DeviceId;

/// A device a client runs on: an ID the client made once and keeps, and what the device is called, such as the
/// computer's name. A client on the hub's machine says it in its hello, and the hub takes a client on the network to be
/// the device it authenticated as. The device that has a chat's conversation is the one that hears and speaks.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ClientDevice {
    pub id: DeviceId,
    pub name: String,
}
