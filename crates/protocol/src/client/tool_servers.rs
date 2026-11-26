//! The requests about the tool servers a hub has of its own, which are MCP servers over HTTP: adding one, editing it
//! and removing it.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{ToolServer, ToolServerAuthEdit, ToolServerAuthGiven, ToolServerId};

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

/// Edits a tool server: what's given here changes, and what's left out stays as it is, so clients that edit different
/// things don't undo each other. Answers with its record, as it is then. What the server has already changes nothing,
/// so sending it again does nothing more. The name, the URL, the header and the key are trimmed.
///
/// A URL whose scheme, host, port or path differs is another server's, as
/// [`ToolServerUrl::is_same_server`](crate::model::ToolServerUrl::is_same_server) says, and what the server knows the
/// person by is never sent there. So one that has a key is moved only with the key the other server is to have. One
/// that's signed in to is moved by signing in there, and any other is signed in to that way too, as
/// [`ToolSignInTo::Edit`](super::tool_sign_ins::ToolSignInTo::Edit) says. The key, or the sign-in, that another takes
/// the place of, or none does, is forgotten once the edit is kept, and isn't revoked at the server.
///
/// Refused as [`ToolServerAdd`] says of each, with `NotFound` when the hub has no record of the server, and with
/// `InvalidRequest` when it would be moved without what it's to know the person by there, or is to keep a key it
/// hasn't. Refused with `Conflict` when the key given is for another server than the one this would leave, as when it
/// has been moved meanwhile.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolServerEdit {
    pub id: ToolServerId,
    /// As the person typed it, for the hub to check, as the URL is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<ToolServerAuthEdit>,
}

impl Request for ToolServerEdit {
    type Reply = ToolServer;

    fn into_request(self) -> ClientRequest {
        ClientRequest::ToolServerEdit(self)
    }
}

/// Has the hub forget a tool server, and its key or its sign-in, and answers with its record as it last was, while the
/// feed says it's gone. It revokes neither at the server. Sending it again is refused with `NotFound`, as is removing one the
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
