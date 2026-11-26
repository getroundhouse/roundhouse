//! The request that checks a tool server as it's to be added or edited, by asking it for its tools, and what came of
//! it. It changes nothing. The client adds or edits the server itself, whatever this is answered with. A client sends it
//! only to a hub from before [`ToolServerSave`](super::tool_server_saves::ToolServerSave), which checks a server as it
//! adds or saves it.

use serde::{Deserialize, Serialize};

use super::tool_servers::{ToolServerAdd, ToolServerEdit};
use super::{ClientRequest, ClientResponse, Reply, Request};

/// Asks a server for its tools, as the hub's own machine reaches it, with what workers would send it once that was
/// done, for at most 15 seconds, and answers with how many it has or why there's no saying. Whether an agent reaches it
/// depends on the machine the agent is on. Nothing is kept or changed, so sending it again asks again.
///
/// Refused at once as [`ToolServerAdd`] and [`ToolServerEdit`] say, before the server is asked anything, and with
/// `Unavailable` when as many are being asked as may be. A key the hub keeps is sent only to the server it was given
/// for.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolServerCheck {
    pub of: ToolServerToCheck,
}

/// What would be done.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolServerToCheck {
    Add(ToolServerAdd),
    Edit(ToolServerEdit),
    /// From a newer client, which the hub refuses.
    #[serde(other)]
    Unknown,
}

impl Request for ToolServerCheck {
    type Reply = ToolServerChecked;

    fn into_request(self) -> ClientRequest {
        ClientRequest::ToolServerCheck(self)
    }
}

/// What came of it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolServerChecked {
    /// The server wasn't asked: it's signed in to, the edit changes nothing of where it is or what it's sent, the hub
    /// has added it already, or has no key it can read to ask with.
    NothingToAsk,
    /// It listed its tools, this many of them where it lists them in pages, which are the first page's.
    Listed {
        tools: u32,
    },
    NotListed {
        why: NoToolList,
    },
    /// From a newer hub.
    #[serde(other)]
    Unknown,
}

impl Reply for ToolServerChecked {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::ToolServerChecked(checked) = response else { return Err(response) };
        Ok(checked)
    }
}

/// Why a server listed no tools. It's of kinds and statuses alone, so it never holds what the server said.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NoToolList {
    /// There was no connecting to it, for this reason where it's known.
    Unreached {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        why: Option<ToolServerUnreached>,
    },
    /// It didn't answer in time.
    TimedOut,
    /// It was connected to, and went before it answered.
    Lost,
    /// It sent the request to another address, which isn't asked anything, since a key goes only where it was given
    /// for.
    Redirected,
    /// It answered 401 or 403 to a request with a key.
    KeyRefused { status: u16 },
    /// It answered 401 to a request without one.
    WantsSignInOrKey,
    /// It answered 403 to a request without one.
    Forbidden,
    /// It answered 404.
    NotFound,
    /// It answered 429.
    TooManyRequests,
    /// It answered with a status that says the problem is its own.
    Failing { status: u16 },
    /// What it answered isn't MCP.
    NotMcp,
    /// Its answer is longer than is read.
    TooLong,
    /// It's MCP, and wouldn't be connected to.
    WontConnect,
    /// It was connected to, and answered a request for its tools with an error, or with none.
    WontList,
    /// From a newer hub.
    #[serde(other)]
    Unknown,
}

/// Why there's no connecting to a server.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolServerUnreached {
    /// Its name isn't found.
    NoSuchName,
    /// Nothing takes connections there.
    ConnectionRefused,
    /// The hub's machine has no way to where it is.
    NoRoute,
    /// The hub's machine doesn't trust its certificate.
    NotTrusted,
    /// What's there doesn't speak `https`, as the URL has it.
    NotHttps,
    /// The hub's machine doesn't say which certificates it trusts.
    NoCertificates,
    /// From a newer hub.
    #[serde(other)]
    Unknown,
}
