//! A wait on a workspace's pull request, which an agent asks for through the worker's tools, and the hub keeps until it
//! ends. It ends once, as against a monitor, which wakes the agent each time.

use serde::{Deserialize, Serialize};

use super::{SessionId, ToolUseId, WaitId, WorkspaceId};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Wait {
    pub id: WaitId,
    pub session_id: SessionId,
    pub workspace_id: WorkspaceId,
    /// The number of the pull request it waits on, which was the branch's when it started.
    pub number: u64,
    /// The agent's call that asked for it, as its CLI names it, so the same call asking again, as a CLI does when it
    /// reconnects, gets the wait it started. `None` when the CLI didn't say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<ToolUseId>,
    pub until: WaitUntil,
    pub started_at_ms: i64,
    /// `None` while it waits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<WaitEnded>,
}

/// What ends a wait, beyond the pull request being done or needing the agent, which always does.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WaitUntil {
    /// Merged or closed, or something only the agent can deal with. *Unconfirmed*
    #[default]
    Done,
    /// Also once its checks have run and none failed, so the agent can then queue it. *Unconfirmed*
    Checks,
    /// One this version doesn't know, from a newer worker, which waits as `Done` does.
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaitEnded {
    pub at_ms: i64,
    pub how: WaitEnd,
}

/// Why a wait ended.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WaitEnd {
    Merged,
    /// Closed without being merged.
    Closed,
    /// It conflicts with its base, outside the merge queue.
    Conflicts,
    ChecksFailed,
    ChangesRequested,
    /// It left the merge queue without merging, or the queue can't merge it.
    LeftQueue,
    /// It's a draft, which can't merge until it's marked ready, for a wait until it's done.
    Draft,
    /// Its checks ran and none failed, for a wait until they did.
    ChecksPassed,
    /// The branch has no pull request any more, or has another one.
    NoPullRequest,
    /// Called off by its agent, or by closing its chat or archiving its workspace.
    Cancelled,
    /// Called off by the person, with Stop on its line.
    CalledOffByPerson,
    /// Called off by a wait its agent started after it, which goes on in its place, since an agent waits on one at a
    /// time.
    Replaced,
    /// One this version doesn't know, from a newer hub.
    #[serde(other)]
    Unknown,
}

impl WaitEnd {
    /// Whether it ended because someone called it off, before what it waited for happened.
    pub fn called_off(self) -> bool {
        match self {
            WaitEnd::Cancelled | WaitEnd::CalledOffByPerson | WaitEnd::Replaced => true,
            WaitEnd::Merged
            | WaitEnd::Closed
            | WaitEnd::Conflicts
            | WaitEnd::ChecksFailed
            | WaitEnd::ChangesRequested
            | WaitEnd::LeftQueue
            | WaitEnd::Draft
            | WaitEnd::ChecksPassed
            | WaitEnd::NoPullRequest
            | WaitEnd::Unknown => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wait_reads_back_as_itself_and_one_from_a_newer_hub_reads_as_unknown() {
        let wait = Wait {
            id: "w".into(),
            session_id: "s".into(),
            workspace_id: "v".into(),
            number: 12,
            call: Some("toolu_1".into()),
            until: WaitUntil::Checks,
            started_at_ms: 5,
            ended: Some(WaitEnded { at_ms: 9, how: WaitEnd::LeftQueue }),
        };
        let json = serde_json::to_string(&wait).unwrap();

        assert_eq!(serde_json::from_str::<Wait>(&json).unwrap(), wait);
        let newer: WaitEnd = serde_json::from_str(r#""reverted""#).unwrap();
        let until: WaitUntil = serde_json::from_str(r#""review""#).unwrap();
        assert_eq!((newer, until), (WaitEnd::Unknown, WaitUntil::Unknown));
    }
}
