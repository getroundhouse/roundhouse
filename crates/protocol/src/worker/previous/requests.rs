//! Protocol 3's requests, in their wire forms.

use serde::Serialize;

use super::launch_identity;
use crate::attachment::AttachmentFile;
use crate::capability::conversation::Start;
use crate::model::{AgentKind, AgentLogin, RunId, Seq, SessionId, SessionSettings, SessionSettingsChange};
use crate::prompt::Prompt;
use crate::worker::{TitleFor, ToolsToken};

/// Protocol 3, the version before [`crate::worker::WORKER_PROTOCOL_VERSION`].
pub const VERSION: u32 = 3;

/// A request as protocol 3 has it, which the hub sends in place of a [`crate::worker::WorkerRequest`], and which is answered
/// with a [`crate::worker::WorkerResponse`] as any is.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum Request {
    AgentStart(AgentStart),
    /// A launch under a login the hub holds, which a worker from before logins refuses rather than starting the agent
    /// under this machine's own.
    AgentStartUnderLogin(AgentStart),
    AgentPrompt(AgentPrompt),
    AgentCancel(AgentCancel),
    AgentCompact(AgentCompact),
    AgentConfigure(AgentConfigure),
    /// As `AgentStartUnderLogin`, for a title.
    TitleUnderLogin(TitleFor),
}

/// A call of protocol 3, as [`crate::worker::WorkerCall`] is of this one.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Call {
    pub id: u64,
    pub request: Request,
}

/// Starts the session's agent, unless it's running already, with the chat's settings as the hub stores them. A run of
/// a worker of protocol 3 says what it claims as it initializes.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentStart {
    pub session_id: SessionId,
    pub run: RunId,
    pub agent: AgentKind,
    pub cwd: String,
    #[serde(flatten, with = "launch_identity")]
    pub start: Start,
    #[serde(flatten)]
    pub settings: SessionSettings,
    pub env: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools_token: Option<ToolsToken>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login: Option<AgentLogin>,
}

impl AgentStart {
    /// `spec` as protocol 3 has it, with the chat's settings as the hub keeps them, `settings`, in place of those it
    /// names.
    pub fn of(spec: crate::worker::AgentSpec, settings: SessionSettings) -> Self {
        let crate::worker::AgentSpec { session_id, run, agent, cwd, start, settings: _, env, tools_token, login } =
            spec;
        Self { session_id, run, agent, cwd, start, settings, env, tools_token, login }
    }

    /// It as protocol 3 sends it, under a login when it has one.
    pub fn into_request(self) -> Request {
        if self.login.is_some() { Request::AgentStartUnderLogin(self) } else { Request::AgentStart(self) }
    }
}

/// Each of the prompt's files has been written into the worktree first. `seq` is where it's recorded, which the
/// agent's [`crate::event::EventBody::PromptTaken`] names.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AgentPrompt {
    pub session_id: SessionId,
    #[serde(rename = "prompt")]
    pub seq: Seq,
    #[serde(flatten)]
    pub prompt: Prompt<AttachmentFile>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AgentCancel {
    pub session_id: SessionId,
}

/// Asks the agent to compact its conversation. `asked` is where that's recorded.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AgentCompact {
    pub session_id: SessionId,
    pub asked: Seq,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keeping: Option<String>,
}

/// A setting left out is left as it is.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AgentConfigure {
    pub session_id: SessionId,
    #[serde(flatten)]
    pub options: SessionSettingsChange,
}
