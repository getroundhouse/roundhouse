//! What a worker asks its hub about the workspace's pull request, and about an agent's waits on it.

use serde::{Deserialize, Serialize};

use super::{Answer, Ask, AskReply, WorkerAsk};
use crate::model::{OwnPullRequest, Wait, WaitId};

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
    /// What the hub knows of it, with how the worktree stands against it. `None` when the branch has none.
    pub pull_request: Option<OwnPullRequest>,
}

impl Answer for PullRequestAnswer {
    fn read(reply: AskReply) -> Result<Self, AskReply> {
        let AskReply::PullRequest(answer) = reply else { return Err(reply) };
        Ok(answer)
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
    /// The workspace's own pull request, with how the worktree stands against it, for a wait on that. A wait on pull
    /// requests it watches has none, since its watch carries what it read of them.
    pub pull_request: Option<OwnPullRequest>,
}

impl Answer for WaitAnswer {
    fn read(reply: AskReply) -> Result<Self, AskReply> {
        let AskReply::Wait(answer) = reply else { return Err(reply) };
        Ok(answer)
    }
}
