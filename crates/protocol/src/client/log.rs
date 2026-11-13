//! The requests that read what a session's log holds, without following it: its events by `seq`, or the agent's own or
//! one subagent's by place, its prompts and its compactions a line each, and which events hold some text. Each only
//! reads.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::event::SessionEvent;
use crate::find::Found;
use crate::model::{CompactionLine, Place, PromptLine, Seq, SessionId, ToolUseId};

/// Reads events from `from_seq` on. Returns at least one event if any exist, and stops at `limit` events or about
/// `max_bytes`, so a huge event can't produce an unbounded reply.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionRead {
    pub id: SessionId,
    pub from_seq: Seq,
    pub limit: u32,
    pub max_bytes: Option<u64>,
    #[serde(default)]
    pub include_raw: bool,
}

impl Request for SessionRead {
    type Reply = Events;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionRead(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Events {
    pub events: Vec<SessionEvent>,
    /// The session's newest event, so a reader knows how far the log goes.
    pub last_seq: Seq,
}

impl Reply for Events {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Events(events) = response else { return Err(response) };
        Ok(events)
    }
}

/// Reads the agent's own events, or those of the subagent `subagent` started, from `from_place` on, as
/// [`SessionRead`] reads every event. A hub older than this refuses it as unsupported, and a client reads by `seq`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionReadPlaces {
    pub id: SessionId,
    pub subagent: Option<ToolUseId>,
    pub from_place: Place,
    pub limit: u32,
    pub max_bytes: Option<u64>,
    #[serde(default)]
    pub include_raw: bool,
}

impl Request for SessionReadPlaces {
    type Reply = PlacedEvents;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionReadPlaces(self)
    }
}

/// The agent's own events, or one subagent's.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PlacedEvents {
    pub events: Vec<SessionEvent>,
    /// The place of the newest of them, so a reader knows how far they go. 0 when there are none.
    pub last_place: Place,
}

impl Reply for PlacedEvents {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::PlacedEvents(events) = response else { return Err(response) };
        Ok(events)
    }
}

/// Every prompt recorded in the session, oldest first, each as one line, for finding one in a long chat without
/// reading the whole log. A hub older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionPromptList {
    pub id: SessionId,
}

impl Request for SessionPromptList {
    type Reply = Prompts;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionPromptList(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Prompts {
    pub prompts: Vec<PromptLine>,
}

impl Reply for Prompts {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Prompts(prompts) = response else { return Err(response) };
        Ok(prompts)
    }
}

/// Every compaction of the agent's own conversation that compacted, oldest first, each as one line, for marking them
/// in a long chat without reading the whole log. A hub older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionCompactionList {
    pub id: SessionId,
}

impl Request for SessionCompactionList {
    type Reply = Compactions;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionCompactionList(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Compactions {
    pub compactions: Vec<CompactionLine>,
}

impl Reply for Compactions {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Compactions(compactions) = response else { return Err(response) };
        Ok(compactions)
    }
}

/// Which of the agent's own events, or those of the subagent `subagent` started, hold `query`, by place, as
/// [`crate::find::Needle::is_in_event`] says, for finding text in a long chat without reading the whole log. An empty
/// query, or one longer than [`crate::find::Needle::MOST_CHARS`], is refused as invalid, and a hub older than this
/// refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionFind {
    pub id: SessionId,
    pub subagent: Option<ToolUseId>,
    pub query: String,
}

impl Request for SessionFind {
    type Reply = Found;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionFind(self)
    }
}

impl Reply for Found {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Found(found) = response else { return Err(response) };
        Ok(found)
    }
}
