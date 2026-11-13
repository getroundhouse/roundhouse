//! The requests about a session's record: making one, listing and reading them, what it runs with, closing and
//! reopening it, how far it's been read, its title, and whose account its agent runs under.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{self, AgentKind, ClientRequestId, Seq, Session, SessionId, SessionSettingsChange, WorkspaceId};

/// Sending the same `client_request_id` again returns the session it made. It's refused for a kind of agent the hub
/// doesn't run, and for an effort or an approval mode the agent isn't offered, as
/// [`AgentDescription::takes`](crate::model::AgentDescription::takes) says.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionCreate {
    pub workspace_id: WorkspaceId,
    pub agent: AgentKind,
    /// What it starts with, in place of what the agent's description says a chat runs with until another is chosen.
    /// One left out is that.
    #[serde(flatten)]
    pub options: SessionSettingsChange,
    #[serde(default)]
    pub client_request_id: Option<ClientRequestId>,
}

impl Request for SessionCreate {
    type Reply = Session;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionCreate(self)
    }
}

/// A session's record as it is then, which [`SessionGet`] reads, and which most requests that make a session or set
/// something on it, such as its settings, title, draft or queue, are answered with. A prompt, or what's asked of the
/// agent, is answered otherwise.
impl Reply for Session {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Session(session) = response else { return Err(response) };
        Ok(session)
    }
}

/// The workspace's newest open session, or a new one if it has none. Clients opening a workspace at the same moment
/// end up in the same session, rather than one each.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionEnsure {
    pub workspace_id: WorkspaceId,
    pub agent: AgentKind,
}

impl Request for SessionEnsure {
    type Reply = Session;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionEnsure(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionList {
    pub workspace_id: WorkspaceId,
}

impl Request for SessionList {
    type Reply = Sessions;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionList(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Sessions {
    pub sessions: Vec<Session>,
}

impl Reply for Sessions {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Sessions(sessions) = response else { return Err(response) };
        Ok(sessions)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionGet {
    pub id: SessionId,
}

impl Request for SessionGet {
    type Reply = Session;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionGet(self)
    }
}

/// Changes the model, approval mode or effort, on the running agent too if there is one. A setting left out is left as
/// it is. Fails if the agent refuses, saving nothing, though a running agent may already have taken the settings it was
/// given before the one it refused. It's refused, changing nothing, for an effort or a mode a new chat would be
/// refused. With no agent running, a change of mode goes in the log as an `AgentMode`, as a running agent's report of
/// one does. Sent again, it sets the same again.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionConfigure {
    pub id: SessionId,
    #[serde(flatten)]
    pub options: SessionSettingsChange,
}

impl Request for SessionConfigure {
    type Reply = Session;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionConfigure(self)
    }
}

/// Stops its agent and hides it from the tabs. The history stays, and it can be reopened. Closing it again leaves it
/// closed. A session nothing has been sent in, with nothing in its log and nothing queued, is deleted instead, with any
/// files added to it, since there's nothing to reopen: the reply is the session as it was, the feed says it's removed,
/// and closing it again is refused with `NotFound`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionClose {
    pub id: SessionId,
}

impl Request for SessionClose {
    type Reply = Session;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionClose(self)
    }
}

/// Reopening an open session leaves it open.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionReopen {
    pub id: SessionId,
}

impl Request for SessionReopen {
    type Reply = Session;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionReopen(self)
    }
}

/// Marks the session read through `through_seq`, so only a reply after it is unread. It never reaches past the newest
/// event, so a reply still to come stays unread. Marking read what already is changes nothing, so sending it again does
/// nothing more.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionMarkRead {
    pub id: SessionId,
    pub through_seq: Seq,
}

impl Request for SessionMarkRead {
    type Reply = Session;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionMarkRead(self)
    }
}

/// Marks the newest reply unread again. With no reply, or one that's already unread, it changes nothing, so sending it
/// again does nothing more.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionMarkUnread {
    pub id: SessionId,
}

impl Request for SessionMarkUnread {
    type Reply = Session;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionMarkUnread(self)
    }
}

/// Sets the title someone typed, as [`WorkspaceRename`](super::workspaces::WorkspaceRename) does for a workspace.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionRename {
    pub id: SessionId,
    pub title: String,
}

impl Request for SessionRename {
    type Reply = Session;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionRename(self)
    }
}

/// Whose account the session's agent runs under, read from the session's worker when asked, and what the hub has heard
/// of that account's limits. It records nothing, so asking again only reads again, and like any read from a worker it
/// starts the workspace's worker if none is running. Refused unless the workspace is ready. A hub older than this
/// refuses it as unsupported.
///
/// Kept for clients from before accounts were records. A client now reads the session's `account` and that account's
/// record, which the feed announces.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionAccount {
    pub id: SessionId,
}

impl Request for SessionAccount {
    type Reply = model::SessionAccount;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionAccount(self)
    }
}

impl Reply for model::SessionAccount {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::SessionAccount(account) = response else { return Err(response) };
        Ok(account)
    }
}
