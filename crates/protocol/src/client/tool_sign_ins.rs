//! The requests that sign in to a tool server: starting a sign-in, finishing it and cancelling it.
//!
//! The client opens the page a start is answered with, in the browser on its own device, and listens there on 127.0.0.1
//! for where that sends the person back. So a sign-in under way is of use only to the client that started it, and the hub
//! keeps it in memory and tells no client of it. One the hub has no more, as once it has stopped, is started again.

use std::net::Ipv4Addr;

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{ToolServer, ToolServerId, ToolSignInId};

crate::tokens!(
    /// The address of the page a person approves a sign-in to a tool server on, which has the sign-in's state in it.
    ToolSignInPage,
    /// What an authorization server sent the person back with: the whole query of the address, which has a code in it
    /// that's good for tokens once.
    ToolSignInReturned,
);

/// The ports a client listens on for where a person is sent back, taking the first that's free. An authorization server
/// may take only a redirect the client document lists, port and all, so the document lists each, and these can't change
/// while a build that listens on them is in use.
pub const REDIRECT_PORTS: [u16; 3] = [17867, 17868, 17869];

/// The path it listens for them at.
pub const REDIRECT_PATH: &str = "/callback";

/// Where a person is sent back to a client listening on `port`, as [`ToolSignInStart::redirect`] is given it.
pub fn redirect(port: u16) -> String {
    format!("http://{}:{port}{REDIRECT_PATH}", Ipv4Addr::LOCALHOST)
}

/// Starts signing in to a tool server, and answers with the page to open, or with why there's none. Nothing is added or
/// changed until the sign-in is finished. The hub asks the server, and then its authorization server, which can take as
/// long as 30 seconds.
///
/// `id` is the client's own, so sending it again is the same sign-in, and answers with the same page while it waits,
/// however the rest of the request differs. It waits 15 minutes.
///
/// Refused as [`ToolServerAdd`](super::tool_servers::ToolServerAdd) says of a name and a URL, and with `Conflict` when
/// the hub has a server of the new one's ID. Refused with `NotFound` when it has no record of the server to sign in to
/// again, and with `InvalidRequest` when that one isn't signed in to. Refused with `Unavailable` when as many sign-ins
/// as may wait are waiting, and when the server or its authorization server can't be reached, which may pass, and with
/// `InvalidRequest` when what they say can't be right.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolSignInStart {
    pub id: ToolSignInId,
    pub to: ToolSignInTo,
    /// Where the client listens, as [`redirect`] makes it.
    pub redirect: String,
}

/// What a sign-in is to.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolSignInTo {
    /// A new server, whose ID is the client's own, and whose name and URL are as the person typed them, for the hub to
    /// check, as the sign-in starts and again as it's finished.
    New { id: ToolServerId, name: String, url: String },
    /// One there is, which is signed in to again, and keeps its ID, its name and its URL.
    Again { id: ToolServerId },
    /// From a newer client, which the hub refuses.
    #[serde(other)]
    Unknown,
}

impl Request for ToolSignInStart {
    type Reply = ToolSignInBegun;

    fn into_request(self) -> ClientRequest {
        ClientRequest::ToolSignInStart(self)
    }
}

/// What a start comes to.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolSignInBegun {
    /// The sign-in waits for the person to approve it on this page.
    Page { page: ToolSignInPage },
    /// The server answers anyone and has no sign-in, so it's one to add with a key, or with neither. Nothing waits.
    NoSignIn,
    /// The server has no way that Roundhouse could sign in to it, and may take a key. Nothing waits.
    NoWay { why: NoToolSignIn },
    /// From a newer hub.
    #[serde(other)]
    Unknown,
}

/// Why a server has no way that Roundhouse could sign in to it. Each address is the server's own word, cut short where
/// it's longer than any address is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NoToolSignIn {
    /// It asks for a sign-in, and doesn't say where, or that doesn't say how.
    NoAuthorizationServer,
    /// Its authorization server takes only clients registered with it, and Roundhouse registers with none.
    OnlyRegisteredClients { authorization_server: String },
    /// What it says of its sign-in is said of another server.
    OtherServer { said: String },
    /// Its authorization server doesn't say it takes PKCE with S256, without which MCP has a client refuse it.
    NoS256 { authorization_server: String },
    /// From a newer hub.
    #[serde(other)]
    Unknown,
}

impl Reply for ToolSignInBegun {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::ToolSignInBegun(begun) = response else { return Err(response) };
        Ok(begun)
    }
}

/// Finishes a sign-in with what the person was sent back with, once they've approved it or refused it, and answers with
/// the server's record. Every client is told of it too.
///
/// Sending it again answers with the server's record as it is then, even after the hub has started again, until the
/// server is signed in to once more or removed. Refused with `NotFound` when the hub has no such sign-in: it has waited
/// as long as one does, was cancelled, ended, or was lost as the hub stopped, so the client starts another.
///
/// Refused with `InvalidRequest`, and the sign-in still waiting, when `returned` isn't of it, as by its `state`. Any
/// other refusal ends it: with `InvalidRequest` when the person refused it, or the authorization server did, with
/// `Unavailable` when that couldn't be reached, and with `Conflict` when another server has taken the name meanwhile, as
/// [`ToolServerAdd`](super::tool_servers::ToolServerAdd) says, or the server being signed in to again has gone.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolSignInFinish {
    pub id: ToolSignInId,
    pub returned: ToolSignInReturned,
}

impl Request for ToolSignInFinish {
    type Reply = ToolServer;

    fn into_request(self) -> ClientRequest {
        ClientRequest::ToolSignInFinish(self)
    }
}

/// Has the hub forget a sign-in that isn't finished. One that's finished stays so, and one the hub doesn't have is
/// forgotten already, so sending it again does nothing more.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolSignInCancel {
    pub id: ToolSignInId,
}

impl Request for ToolSignInCancel {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::ToolSignInCancel(self)
    }
}
