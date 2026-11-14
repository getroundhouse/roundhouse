//! Explicit remote targets, independent of the checked-out branch. New methods prevent older hubs from silently
//! interpreting a plural wait as a legacy single-branch wait.

use serde::{Deserialize, Serialize};

use super::pull_requests::WaitAnswer;
use super::{Answer, Ask, AskReply, WorkerAsk};
use crate::model::{PullRequestSelection, PullRequestsSnapshot, ToolUseId};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReadPullRequests {
    pub pull_requests: PullRequestSelection,
}

impl Ask for ReadPullRequests {
    type Answer = PullRequestsSnapshot;

    fn into_ask(self) -> WorkerAsk {
        WorkerAsk::PullRequests(self)
    }
}

impl Answer for PullRequestsSnapshot {
    fn read(reply: AskReply) -> Result<Self, AskReply> {
        let AskReply::PullRequests(snapshot) = reply else { return Err(reply) };
        Ok(snapshot)
    }
}

/// An instruction, not recorded data: unknown values are refused by the protocol envelope.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestWaitUntil {
    RequiredChecks,
    AllChecks,
    Merged,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StartPullRequestWait {
    pub pull_requests: PullRequestSelection,
    pub until: PullRequestWaitUntil,
    pub timeout_seconds: Option<u64>,
    pub call: Option<ToolUseId>,
}

impl Ask for StartPullRequestWait {
    type Answer = WaitAnswer;

    fn into_ask(self) -> WorkerAsk {
        WorkerAsk::StartPullRequestWait(self)
    }
}
