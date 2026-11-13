//! What a client asks of the hub itself: how it stands, stopping or restarting it, its own settings, and what each
//! kind of agent it runs offers.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{AgentDescription, HubSettings, HubSettingsChange};

/// How the hub stands. Works without a hello, and so does [`HubShutdown`], so a client of any version can see a hub
/// and stop it. Neither can change form, even in a real break.
#[derive(Clone, Copy, Debug)]
pub struct HubStatus;

impl Request for HubStatus {
    type Reply = super::HubStatus;

    fn into_request(self) -> ClientRequest {
        ClientRequest::HubStatus
    }
}

impl Reply for super::HubStatus {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::HubStatus(status) = response else { return Err(response) };
        Ok(status)
    }
}

/// Stops the hub and its workers. Agents are stopped and resume on the next prompt.
#[derive(Clone, Copy, Debug)]
pub struct HubShutdown;

impl Request for HubShutdown {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::HubShutdown
    }
}

/// Stops the hub as [`HubShutdown`] does, but only if nothing would notice, as [`HubStatus::is_idle`] says of the one
/// asking. The hub decides as it stops, so nothing can start between a client seeing it idle and stopping it. One
/// that's busy refuses with `Conflict`, and carries on. Needs a hello, unlike `HubShutdown`.
///
/// [`HubStatus::is_idle`]: super::HubStatus::is_idle
#[derive(Clone, Copy, Debug)]
pub struct HubShutdownIfIdle;

impl Request for HubShutdownIfIdle {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::HubShutdownIfIdle
    }
}

/// Stops the hub without stopping its workers, whose agents carry on, for the next hub to connect to again. It waits
/// for setup scripts that are running and requests under way to finish first. Works without a hello, as
/// [`HubShutdown`] does. Only a hub whose status says [`keeps_workers`](super::HubStatus::keeps_workers) knows it.
#[derive(Clone, Copy, Debug)]
pub struct HubRestart;

impl Request for HubRestart {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::HubRestart
    }
}

/// Changes the hub's own settings, which it keeps and every client shows, and answers with them as they are now. A
/// setting left out is left as it is. Sent again, it sets the same again.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HubConfigure {
    #[serde(flatten)]
    pub settings: HubSettingsChange,
}

impl Request for HubConfigure {
    type Reply = HubSettings;

    fn into_request(self) -> ClientRequest {
        ClientRequest::HubConfigure(self)
    }
}

impl Reply for HubSettings {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::HubSettings(settings) = response else { return Err(response) };
        Ok(settings)
    }
}

/// What each kind of agent the hub runs is called and offers, as [`AgentDescription`] says. It doesn't change while
/// the hub runs, so a client asks once a connection. A hub older than this refuses it as unsupported, and runs only
/// Claude.
#[derive(Clone, Copy, Debug)]
pub struct AgentList;

impl Request for AgentList {
    type Reply = Agents;

    fn into_request(self) -> ClientRequest {
        ClientRequest::AgentList
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Agents {
    /// Each kind of agent the hub runs, in the order a client offers them.
    pub agents: Vec<AgentDescription>,
}

impl Reply for Agents {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Agents(agents) = response else { return Err(response) };
        Ok(agents)
    }
}
