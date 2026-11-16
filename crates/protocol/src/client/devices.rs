//! Pairing a device and reaching a hub over the network.
use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::DeviceId;
use crate::pairing::{Device, DeviceToken, Pairing, PairingCode};
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Paired {
    pub device_id: DeviceId,
    pub token: DeviceToken,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Devices {
    pub devices: Vec<Device>,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Pair {
    pub code: PairingCode,
    pub device_name: String,
}
impl Request for Pair {
    type Reply = Paired;
    fn into_request(self) -> ClientRequest {
        ClientRequest::Pair(self)
    }
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Authenticate {
    pub token: DeviceToken,
}
impl Request for Authenticate {
    type Reply = ();
    fn into_request(self) -> ClientRequest {
        ClientRequest::Authenticate(self)
    }
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PairingStart;
impl Request for PairingStart {
    type Reply = Pairing;
    fn into_request(self) -> ClientRequest {
        ClientRequest::PairingStart
    }
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PairingWait {
    pub code: PairingCode,
}
impl Request for PairingWait {
    type Reply = Device;
    fn into_request(self) -> ClientRequest {
        ClientRequest::PairingWait(self)
    }
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DeviceList;
impl Request for DeviceList {
    type Reply = Devices;
    fn into_request(self) -> ClientRequest {
        ClientRequest::DeviceList
    }
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DeviceRevoke {
    pub id: DeviceId,
}
impl Request for DeviceRevoke {
    type Reply = ();
    fn into_request(self) -> ClientRequest {
        ClientRequest::DeviceRevoke(self)
    }
}
impl Reply for Paired {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Paired(reply) = response else { return Err(response) };
        Ok(reply)
    }
}
impl Reply for Devices {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Devices(reply) = response else { return Err(response) };
        Ok(reply)
    }
}
impl Reply for Pairing {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Pairing(reply) = response else { return Err(response) };
        Ok(reply)
    }
}
impl Reply for Device {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Device(reply) = response else { return Err(response) };
        Ok(reply)
    }
}
