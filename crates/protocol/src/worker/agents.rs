//! What a hub asks of a session's agent: starting it, and what it's given and told once it's running, which the worker
//! answers by saying only whether the agent was there to take it.

use serde::{Deserialize, Serialize};

use super::{Reply, Request, ToolsToken, WorkerRequest, WorkerResponse};
use crate::attachment::AttachmentFile;
use crate::event::ApprovalDecision;
use crate::model::{AgentKind, AgentLogin, ApprovalId, RunId, Seq, SessionId, SessionSettings, SessionSettingsChange};
use crate::prompt::Prompt;

/// Starts the session's agent, unless it's running already.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentSpec {
    pub session_id: SessionId,
    /// Chosen by the hub and unique to this launch. The agent's output carries it, so a late event
    /// or delta from an agent that has been replaced can't be taken for one from the new agent.
    pub run: RunId,
    pub agent: AgentKind,
    pub cwd: String,
    #[serde(flatten, with = "super::launch_identity")]
    pub start: crate::capability::conversation::Start,
    /// What it runs with.
    #[serde(flatten)]
    pub settings: SessionSettings,
    /// Set in the agent's environment, along with the worker's own.
    pub env: Vec<(String, String)>,
    /// What the agent's calls to the worker's tools carry, which tells the worker whose they are. The hub keeps one for
    /// each session, so every launch of its agent has the same. `None` when the worker isn't serving its tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools_token: Option<ToolsToken>,
    /// The login the hub holds for the account the agent runs under, which the agent's adapter gives its CLI, and the
    /// worker keeps only as long as the agent. `None` runs it under this machine's own login. Requests with a
    /// login use `agent_start_under_login`, which an older worker refuses before starting anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login: Option<AgentLogin>,
}

impl Request for AgentSpec {
    type Reply = AgentStarted;

    fn into_request(self) -> WorkerRequest {
        if self.login.is_some() { WorkerRequest::AgentStartUnderLogin(self) } else { WorkerRequest::AgentStart(self) }
    }
}

/// `run` is the one already going if `already_running` is set.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgentStarted {
    pub pid: u32,
    pub run: RunId,
    pub already_running: bool,
}

impl Reply for AgentStarted {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        let WorkerResponse::AgentStarted(started) = response else { return Err(response) };
        Ok(started)
    }
}

/// One capability-checked request. Older workers refuse this envelope as `Unsupported`, before acting on it.
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

/// Each of the prompt's files has been written into the worktree first. `seq` is where it's recorded, which the
/// agent's [`crate::event::EventBody::PromptTaken`] names.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgentPrompt {
    pub session_id: SessionId,
    #[serde(rename = "prompt")]
    pub seq: Seq,
    #[serde(flatten)]
    pub prompt: Prompt<AttachmentFile>,
}

impl Request for AgentPrompt {
    type Reply = Delivery;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::AgentPrompt(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgentApproval {
    pub session_id: SessionId,
    pub approval_id: ApprovalId,
    pub decision: ApprovalDecision,
}

impl Request for AgentApproval {
    type Reply = Delivery;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::AgentApproval(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgentCancel {
    pub session_id: SessionId,
}

impl Request for AgentCancel {
    type Reply = Delivery;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::AgentCancel(self)
    }
}

/// Asks the agent to compact its conversation. `asked` is where that's recorded, which the agent's
/// [`crate::event::EventBody::PromptTaken`] and its compaction name.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgentCompact {
    pub session_id: SessionId,
    pub asked: Seq,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keeping: Option<String>,
}

impl Request for AgentCompact {
    type Reply = Delivery;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::AgentCompact(self)
    }
}

/// A setting left out is left as it is.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AgentConfigure {
    pub session_id: SessionId,
    #[serde(flatten)]
    pub options: SessionSettingsChange,
}

impl Request for AgentConfigure {
    type Reply = Delivery;

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::AgentConfigure(self)
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
}

impl Reply for Delivery {
    fn read(response: WorkerResponse) -> Result<Self, WorkerResponse> {
        if matches!(response, WorkerResponse::Ok) {
            return Ok(Self::Delivered);
        }
        let WorkerResponse::AgentNotRunning = response else { return Err(response) };
        Ok(Self::AgentNotRunning)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Effort, LoginToken, ModelName, PermissionMode};

    fn launch(login: Option<AgentLogin>) -> WorkerRequest {
        AgentSpec {
            session_id: "s".into(),
            run: RunId::new(1),
            agent: AgentKind::Claude,
            cwd: "/w".into(),
            start: crate::capability::conversation::Start::New { conversation: Some("a".into()) },
            settings: SessionSettings {
                model: ModelName::named("opus"),
                permission_mode: PermissionMode::FullAccess,
                effort: Effort::High,
            },
            env: vec![],
            tools_token: None,
            login,
        }
        .into_request()
    }

    #[test]
    fn a_launch_without_a_login_is_as_it_was_before_logins() {
        let sent = serde_json::to_string(&launch(None)).unwrap();

        assert!(!sent.contains("login"), "{sent}");
        assert_eq!(serde_json::from_str::<WorkerRequest>(&sent).unwrap(), launch(None));
    }

    #[test]
    fn a_launch_carries_its_login_and_hides_the_token_from_debug() {
        let with_login = launch(Some(AgentLogin { token: LoginToken::new("a-made-up-token") }));
        let sent = serde_json::to_string(&with_login).unwrap();

        assert!(sent.contains(r#""login":{"token":"a-made-up-token"}"#), "{sent}");
        assert_eq!(serde_json::from_str::<WorkerRequest>(&sent).unwrap(), with_login);
        assert!(!format!("{with_login:?}").contains("made-up"));
    }
    #[test]
    fn an_older_worker_refuses_a_required_login_instead_of_starting_under_ambient_credentials() {
        #[derive(Deserialize, Debug, PartialEq)]
        #[serde(tag = "method", rename_all = "snake_case")]
        enum OlderWorker {
            AgentStart,
            TitleFor,
            #[serde(other)]
            Unknown,
        }
        let login = AgentLogin { token: LoginToken::new("made-up") };
        let requests = [
            launch(Some(login.clone())),
            super::super::TitleFor {
                agent: AgentKind::Claude,
                prompt: "title".into(),
                images: vec![],
                login: Some(login),
            }
            .into_request(),
        ];
        for request in requests {
            let wire = serde_json::to_string(&request).unwrap();
            assert_eq!(serde_json::from_str::<OlderWorker>(&wire).unwrap(), OlderWorker::Unknown);
        }
        let wire = serde_json::to_string(&launch(None)).unwrap();
        assert_eq!(serde_json::from_str::<OlderWorker>(&wire).unwrap(), OlderWorker::AgentStart);
    }
}
