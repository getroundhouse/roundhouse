//! What a hub asks of a session's agent: starting it, and what it's given and told once it's running, which the worker
//! answers by saying only whether the agent was there to take it.

use serde::{Deserialize, Serialize};

use super::{Reply, Request, ToolsToken, WorkerRequest, WorkerResponse};
use crate::capability::Capabilities;
use crate::capability::conversation::Start;
use crate::capability::settings::Settings;
use crate::model::{AgentKind, AgentLogin, GithubRepositoryName, InteractionMode, RunId, SessionId};

/// Starts the session's agent, unless it's running already.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentSpec {
    pub session_id: SessionId,
    /// Chosen by the hub and unique to this launch. The agent's output carries it, so a late event
    /// or delta from an agent that has been replaced can't be taken for one from the new agent.
    pub run: RunId,
    pub agent: AgentKind,
    pub cwd: String,
    /// What conversation it starts from.
    #[serde(flatten)]
    pub start: Start,
    /// What it runs with: its model, and each other setting its kind claims, as the hub chose for it from the chat's.
    pub settings: Settings,
    /// Set in the agent's environment, along with the worker's own.
    pub env: Vec<(String, String)>,
    /// What the agent's calls to the worker's tools carry, which tells the worker whose they are. The hub keeps one for
    /// each session, so every launch of its agent has the same. `None` when the worker isn't serving its tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools_token: Option<ToolsToken>,
    /// The login the hub holds for the account the agent runs under, which the agent's adapter gives its CLI, and the
    /// worker keeps only as long as the agent. `None` runs it under this machine's own login.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<AgentLogin>,
    /// The workspace's repository on GitHub, as its address names it, which the agent is told so it links to its issues
    /// and pull requests by their real addresses. `None` for one that isn't on github.com.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<GithubRepositoryName>,
    /// How the person talks with the agent as it starts, which an adapter that says lines for it, as
    /// [`crate::capability::output::CoverLine`] says, needs to know, and is told as it changes with
    /// [`crate::capability::Control::Interaction`]. Typing is left out, as in every launch from before.
    #[serde(default, skip_serializing_if = "InteractionMode::is_typing")]
    pub interaction: InteractionMode,
}

impl Request for AgentSpec {
    type Reply = AgentStarted;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::AgentStart(self)
    }
}

/// `run` is the one already going if `already_running` is set.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgentStarted {
    pub pid: u32,
    pub run: RunId,
    pub already_running: bool,
    /// What the run claims, as its adapter says, which the hub holds it to.
    pub capabilities: Box<Capabilities>,
}

impl Reply for AgentStarted {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::AgentStarted(started) = response else { return Err(response) };
        Ok(started)
    }
}

/// One capability-checked request. A worker refuses one its run doesn't offer as `Unsupported`, before acting on it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgentControl {
    pub session_id: SessionId,
    #[serde(flatten)]
    pub control: crate::capability::Control,
}

impl Request for AgentControl {
    type Reply = Delivery;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::AgentControl(self)
    }
}

/// Resolves once the process has exited.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgentStop {
    pub session_id: SessionId,
}

impl Request for AgentStop {
    type Reply = Delivery;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::AgentStop(self)
    }
}

/// Whether the session's agent was there to take what it was sent. What comes of it reaches the hub as what the agent
/// says, rather than in the reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// It took it, or for [`AgentStop`], it stopped.
    Delivered,
    /// The session has no running agent. The hub starts one and sends the request again.
    AgentNotRunning,
    /// The agent won't take a prompt while its turn runs, as when the turn can't be steered, so it took nothing. The
    /// hub keeps the prompt and sends it again once the turn ends. A worker from before this fails the request instead.
    AfterTheTurn,
}

impl Reply for Delivery {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        if matches!(response, WorkerResponse::Ok) {
            return Ok(Self::Delivered);
        }
        if matches!(response, WorkerResponse::AfterTheTurn) {
            return Ok(Self::AfterTheTurn);
        }
        let WorkerResponse::AgentNotRunning = response else { return Err(response) };
        Ok(Self::AgentNotRunning)
    }
}
