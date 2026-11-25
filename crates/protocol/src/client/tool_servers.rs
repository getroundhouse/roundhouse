//! The requests about the tool servers a hub has of its own, which are MCP servers over HTTP: adding one and removing
//! it.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{ToolServer, ToolServerAuthGiven, ToolServerId};

/// Adds a tool server, and answers with its record. The hub keeps its key, if it has one, in a file of its own, and
/// never sends it to a client. The hub asks the server nothing, since whether it can be reached depends on the machine
/// an agent is on. The name, the URL, the header and the key are trimmed.
///
/// `id` is the client's own, so sending it again is the same server, and answers with its record as it is then, however
/// the rest of the request differs. Once the server has been removed, the same `id` adds a new one.
///
/// Refused with `InvalidRequest` when the name, the URL or the header can't be one, as
/// [`ToolServerName::typed`](crate::model::ToolServerName::typed),
/// [`ToolServerUrl::typed`](crate::model::ToolServerUrl::typed) and
/// [`ToolServerHeader::typed`](crate::model::ToolServerHeader::typed) say, when the key can't be sent in a header, and
/// when this hub doesn't know the way the server is to know the person. Refused with `Conflict` when agents would call
/// it what they call another server, as [`ToolServerName::for_agents`](crate::model::ToolServerName::for_agents) says,
/// in any case. A URL another server has isn't refused, since two can be for two accounts at it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolServerAdd {
    pub id: ToolServerId,
    /// As the person typed it, for the hub to check, as the URL is.
    pub name: String,
    pub url: String,
    pub auth: ToolServerAuthGiven,
}

impl Request for ToolServerAdd {
    type Reply = ToolServer;

    fn into_request(self) -> ClientRequest {
        ClientRequest::ToolServerAdd(self)
    }
}

/// What a request about a tool server is answered with: its record, as it is then, or as it last was once it's been
/// removed.
impl Reply for ToolServer {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::ToolServer(server) = response else { return Err(response) };
        Ok(server)
    }
}

/// Has the hub forget a tool server, and its key, and answers with its record as it last was, while the feed says it's
/// gone. It doesn't revoke the key at the server. Sending it again is refused with `NotFound`, as is removing one the
/// hub has no record of.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolServerRemove {
    pub id: ToolServerId,
}

impl Request for ToolServerRemove {
    type Reply = ToolServer;

    fn into_request(self) -> ClientRequest {
        ClientRequest::ToolServerRemove(self)
    }
}
