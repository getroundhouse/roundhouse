//! What a hub says of its connection to a worker: the hello that starts it, which agents the worker is running, for a
//! hub that connects again, what the hub has recorded of what the worker sent, and that it means the worker to stop.

use serde::{Deserialize, Serialize};

use super::{Reply, Request, WorkerRequest, WorkerResponse};
use crate::model::{RunId, SessionId};

/// It names the newest version of the protocol the hub speaks, and the oldest, which a hub from before ranges leaves
/// out, since it speaks only the one.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Hello {
    pub protocol_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oldest_protocol_version: Option<u32>,
    /// The hub asks with [`Stop`] when it means the worker to stop, so a worker that loses it without being asked
    /// keeps its agents running for the next hub, and numbers what it sends, so the next one gets what this one didn't
    /// record. A hub from before this stops a worker by closing the connection, so a worker it didn't say this to
    /// stops when it loses it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep_running_without_hub: bool,
}

impl Request for Hello {
    type Reply = HelloReply;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::Hello(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HelloReply {
    /// The one chosen: the newest both sides speak.
    pub protocol_version: u32,
    /// Which build the worker is, as [`crate::Build::version`] says, so a hub can move a worker from another build
    /// onto its own once nothing is at work in its workspace. `None` from a worker from before this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
}

impl Reply for HelloReply {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::Hello(hello) = response else { return Err(response) };
        Ok(hello)
    }
}

/// Stops the worker's agents and setup script once the hub closes the connection, and then the worker, as losing a
/// hub that didn't say [`Hello::keep_running_without_hub`] does. Answered at once.
#[derive(Clone, Copy, Debug)]
pub struct Stop;

impl Request for Stop {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::Stop
    }
}

/// The agents running, so a hub that connects again carries on with their sessions rather than taking them for
/// stopped.
#[derive(Clone, Copy, Debug)]
pub struct RunningAgents;

impl Request for RunningAgents {
    type Reply = RunningAgentsReply;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::RunningAgents
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RunningAgentsReply {
    pub agents: Vec<RunningAgent>,
}

impl Reply for RunningAgentsReply {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::RunningAgents(running) = response else { return Err(response) };
        Ok(running)
    }
}

/// An agent a worker is running, as [`RunningAgentsReply`] lists it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RunningAgent {
    pub session_id: SessionId,
    pub run: RunId,
    /// What the run claims, as its launch's reply said, so a hub that carries on with it holds it to them. `None` from
    /// a worker of protocol 3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Box<crate::capability::Capabilities>>,
}

impl RunningAgent {
    /// What its run claims, as a session's record holds them, if its worker said.
    pub fn claims(&self) -> Option<crate::model::RunCapabilities> {
        let claims = self.capabilities.as_deref()?;
        Some(crate::model::RunCapabilities::new(self.run, claims.clone()))
    }
}

/// What the hub has recorded, through the frame numbered `through`, so the worker can stop keeping it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Received {
    pub through: u64,
}

impl Request for Received {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::Received(self)
    }
}
