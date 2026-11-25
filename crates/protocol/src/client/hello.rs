//! The hello that starts a connection, in which a client and a hub find a version of the protocol they share.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::versions::Versions;

/// Must come first on a connection. It names the newest version of the protocol the client speaks, and the oldest,
/// which a client from before ranges leaves out, since it speaks only the one, and which device the client runs on,
/// which a client from before devices leaves out, as does one with no device of its own, such as the command line.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Hello {
    pub protocol_version: u32,
    pub client: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oldest_protocol_version: Option<u32>,
    /// Which device it runs on. The hub takes a client on the network to be the device it authenticated as, whatever
    /// this says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<crate::model::ClientDevice>,
}

impl Request for Hello {
    type Reply = HelloReply;

    fn into_request(self) -> ClientRequest {
        ClientRequest::Hello(self)
    }
}

/// What a hub says of itself as it refuses a hello that names no version it speaks, so the client can say which of the
/// two is older. It never changes form, as the hellos don't, since it's read across a break.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HubRefusing {
    /// Its release, as [`HelloReply`] says it.
    pub version: String,
    /// The versions of the protocol it speaks.
    pub speaks: Versions,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HelloReply {
    /// The one chosen: the newest both sides speak.
    pub protocol_version: u32,
    pub hub_version: String,
}

impl Reply for HelloReply {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Hello(hello) = response else { return Err(response) };
        Ok(hello)
    }
}
