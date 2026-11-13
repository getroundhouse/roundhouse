//! What a worker asks its hub about the workspace's pull request, and about an agent's waits on it.

use serde::{Deserialize, Serialize};

use super::{Answer, Ask, AskReply, WorkerAsk};
use crate::model::{PullRequest, ToolUseId, Wait, WaitId, WaitUntil};

/// The workspace's pull request, as the hub reads it for an agent.
#[derive(Clone, Copy, Debug)]
pub struct ReadPullRequest;

impl Ask for ReadPullRequest {
    type Answer = PullRequestAnswer;

    fn into_ask(self) -> WorkerAsk {
        WorkerAsk::PullRequest
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PullRequestAnswer {
    /// What the hub knows of it. `None` when the branch has none.
    pub pull_request: Option<PullRequest>,
}

impl Answer for PullRequestAnswer {
    fn read(reply: AskReply) -> Result<Self, AskReply> {
        let AskReply::PullRequest(answer) = reply else { return Err(reply) };
        Ok(answer)
    }
}

/// Starts a wait on the workspace's pull request, for the agent's call `call`, from where it stands now. A branch
/// without one is refused.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StartWait {
    #[serde(default)]
    pub until: WaitUntil,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<ToolUseId>,
}

impl Ask for StartWait {
    type Answer = WaitAnswer;

    fn into_ask(self) -> WorkerAsk {
        WorkerAsk::StartWait(self)
    }
}

/// How one of the session's waits stands.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReadWait {
    pub wait: WaitId,
}

impl Ask for ReadWait {
    type Answer = WaitAnswer;

    fn into_ask(self) -> WorkerAsk {
        WorkerAsk::Wait(self)
    }
}

/// Ends one of the session's waits, as cancelled, unless it has ended already.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CancelWait {
    pub wait: WaitId,
}

impl Ask for CancelWait {
    type Answer = WaitAnswer;

    fn into_ask(self) -> WorkerAsk {
        WorkerAsk::CancelWait(self)
    }
}

/// A wait as it stands, and the pull request as the hub last read it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WaitAnswer {
    pub wait: Wait,
    pub pull_request: Option<PullRequest>,
}

impl Answer for WaitAnswer {
    fn read(reply: AskReply) -> Result<Self, AskReply> {
        let AskReply::Wait(answer) = reply else { return Err(reply) };
        Ok(answer)
    }
}
