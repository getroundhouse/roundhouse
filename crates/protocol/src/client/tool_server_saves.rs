//! The request that adds a tool server, or saves an edit of one, once the hub has asked it for its tools, and the
//! request that lets go of one while the server is asked. So the hub keeps the rule that a server that doesn't list its
//! tools isn't added or saved, unless the person has it done anyway, for every client.

use serde::{Deserialize, Serialize};

use super::tool_server_checks::{NoToolList, ToolServerToCheck};
use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{ToolServer, ToolServerSaveId};

/// Adds a tool server, or saves an edit of one, as `of` says, and answers with its record. Where `when` is
/// [`SaveWhen::Listed`], the hub first asks the server for its tools, as
/// [`ToolServerCheck`](super::tool_server_checks::ToolServerCheck) does, with what workers would send it, for at most
/// 15 seconds, and adds or saves it only once it lists them. It asks nothing where that says there's nothing to ask,
/// as of a server that's signed in to, or of an edit of the name alone. A server that doesn't list them is answered
/// with why, and nothing changes. An edit's own `saved_anyway` is ignored: `when` says.
///
/// Refused at once, before the server is asked anything, as [`ToolServerAdd`](super::tool_servers::ToolServerAdd) and
/// [`ToolServerEdit`](super::tool_servers::ToolServerEdit) say, and with `Unavailable` when as many servers are being
/// asked as may be, checks among them. No lock is held while the server is asked, so the hub takes what's to be saved
/// again once it has answered. That's refused as the add or the edit would be then, as when the server has been
/// removed or its name taken meanwhile, and with `Conflict` when the hub would now ask anything else of a server, as
/// when the server has been moved, or given another key, meanwhile.
///
/// Answered with [`ToolServerSaved::Cancelled`], and nothing changed, when it's cancelled, or its connection closes,
/// before the hub has begun to save it. One that has begun is saved whatever comes after.
///
/// Sending it again adds or saves nothing more, since an add is of the server's own ID and an edit changes only what
/// differs. Refused with `Conflict` while a save of the same `id` is under way.
///
/// A hub from before it refuses it with `Unsupported`, and a client then says that the hub has to be updated.
///
/// *Unconfirmed:* this request's shape, and that a save the hub has begun is saved whatever comes after.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolServerSave {
    pub id: ToolServerSaveId,
    pub of: ToolServerToCheck,
    pub when: SaveWhen,
}

/// When a server is added or saved.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SaveWhen {
    /// Once it has listed its tools, or at once where there's nothing to ask it.
    Listed,
    /// At once, without asking it anything, as the person chose once it didn't list them. An edit saved so has running
    /// agents list the server's tools again, as `ToolServerEdit`'s `saved_anyway` says.
    Anyway,
    /// From a newer client, which the hub refuses.
    #[serde(other)]
    Unknown,
}

impl Request for ToolServerSave {
    type Reply = ToolServerSaved;

    fn into_request(self) -> ClientRequest {
        ClientRequest::ToolServerSave(self)
    }
}

/// What a save came to.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolServerSaved {
    /// It was added, or the edit was saved, and this is its record as it is then.
    Saved { server: ToolServer },
    /// It didn't list its tools, for this reason, so nothing changed.
    NotListed { why: NoToolList },
    /// It was cancelled, or its connection closed, before the hub began to save it, so nothing changed.
    Cancelled,
    /// From a newer hub.
    #[serde(other)]
    Unknown,
}

impl Reply for ToolServerSaved {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::ToolServerSaved(saved) = response else { return Err(response) };
        Ok(saved)
    }
}

/// Lets go of the save `id` while its server is asked, so nothing is added or saved. One the hub has begun to save is
/// saved all the same, and its own reply says so. One the hub doesn't have has ended already, so sending it again does
/// nothing more.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolServerSaveCancel {
    pub id: ToolServerSaveId,
}

impl Request for ToolServerSaveCancel {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::ToolServerSaveCancel(self)
    }
}
