//! The hub's tool servers as a hub sets them on a worker, which passes its agents' calls on to them.

use serde::{Deserialize, Serialize};

use super::{Request, WorkerRequest};
use crate::capability::setup::ServerName;
use crate::model::{ToolServerHeader, ToolServerId, ToolServerName, ToolServerUrl};

/// Sets the tool servers the worker's agents reach through it, all of them each time, so sending it twice is the same
/// as once. An agent started after the answer is given every server in it, and a call to one that's no longer in it is
/// told it was removed. One to a server that's another now, by its URL's
/// [`place`](crate::model::ToolServerUrl::place), is refused until its agent connects to it afresh, so what was meant
/// for one server is never sent to another. A call already under way goes on as it began.
///
/// The worker keeps them in memory and in no file, so each hub that connects sets them again. It applies a set on its
/// connection's own loop, before it reads the next frame, as it does a
/// [`GitCredentialSet`](super::GitCredentialSet), so sets apply in the order the hub sent them. It has nothing to
/// refuse one for, since a server whose URL or header can't be used fails only the calls made to it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ToolServersSet {
    pub servers: Vec<ProxiedServer>,
}

impl Request for ToolServersSet {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::ToolServersSet(self)
    }
}

/// A tool server of the hub's, an MCP server over HTTP, as a worker passes calls on to it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProxiedServer {
    /// Names its path on the worker, so an agent that's running reaches it whatever else of it changes, such as its
    /// header.
    pub id: ToolServerId,
    /// What agents are given it as, which the hub makes with [`ToolServerName::for_agents`].
    pub name: ServerName,
    /// What the person calls it, such as `Linear (Northwind)`, for what an agent is told when a call fails.
    pub label: ToolServerName,
    pub url: ToolServerUrl,
    /// What each request to it carries. `None` for a server that needs no key, and for one that needs signing in to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<ServerHeader>,
    /// How its sign-in stands. `None` for a server that isn't signed in to. A worker that doesn't know of it sends what
    /// the header says, and passes on as a refusal what the server makes of that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign_in: Option<ServerSignIn>,
}

/// How a server's sign-in stands, for the worker that passes calls on to it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ServerSignIn {
    /// The server's header carries its access token `token`. A worker whose server refuses it asks for another, as
    /// [`RenewToolServer`](super::RenewToolServer) says.
    Works { token: TokenNumber },
    /// The person has to sign in, so the worker answers each call itself, and says so.
    NeedsSignIn,
    /// What a newer hub says. Calls go with the header, if there's one, and no other is asked for.
    #[serde(other)]
    Unknown,
}

/// Which of a server's access tokens one is. Each its sign-in is given has a higher one, through signing in to it again
/// too, so it also says whether what the hub has is what it had.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(transparent)]
pub struct TokenNumber(u64);

impl TokenNumber {
    pub const FIRST: TokenNumber = TokenNumber(1);

    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

/// A header by which a server knows the person, such as `Authorization`, and all of its value.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ServerHeader {
    pub name: ToolServerHeader,
    pub value: ServerHeaderValue,
}

impl ServerHeader {
    /// The header a server that's signed in to knows the person by, with `access_token`.
    pub fn bearer(access_token: &str) -> Self {
        let value = ServerHeaderValue::new(format!("Bearer {access_token}"));
        Self { name: ToolServerHeader::authorization(), value }
    }
}

crate::tokens!(
    /// The value of a tool server's header, as it's sent, such as `Bearer` and a key. The worker sends it only to the
    /// server's own origin, and no agent is given it.
    ServerHeaderValue,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_reads_back_as_it_was_set_with_a_header_and_without() {
        let header = ServerHeader {
            name: ToolServerHeader::typed("Authorization").unwrap(),
            value: ServerHeaderValue::new("Bearer made-up"),
        };
        let server = ProxiedServer {
            id: "t-1".into(),
            name: "linear_northwind".into(),
            label: ToolServerName::typed("Linear (Northwind)").unwrap(),
            url: ToolServerUrl::typed("https://mcp.example.com/mcp").unwrap(),
            header: Some(header),
            sign_in: None,
        };
        let keyless = ProxiedServer { header: None, ..server.clone() };
        let works = Some(ServerSignIn::Works { token: TokenNumber::FIRST });
        let signed_in = ProxiedServer { sign_in: works, ..server.clone() };
        let needs = ProxiedServer { header: None, sign_in: Some(ServerSignIn::NeedsSignIn), ..server.clone() };
        let set = ToolServersSet { servers: vec![server, keyless, signed_in, needs] }.into_request();

        let written = serde_json::to_string(&set).unwrap();

        assert!(written.starts_with(r#"{"method":"tool_servers_set","servers":[{"id":"t-1","#), "{written}");
        assert_eq!(serde_json::from_str::<WorkerRequest>(&written).unwrap(), set);
    }
}
