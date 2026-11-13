//! The hello that starts a connection, in which a client and a hub find a version of the protocol they share.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};

/// Must come first on a connection. It names the newest version of the protocol the client speaks, and the oldest,
/// which a client from before ranges leaves out, since it speaks only the one.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Hello {
    pub protocol_version: u32,
    pub client: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oldest_protocol_version: Option<u32>,
}

impl Request for Hello {
    type Reply = HelloReply;

    fn into_request(self) -> ClientRequest {
        ClientRequest::Hello(self)
    }
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
