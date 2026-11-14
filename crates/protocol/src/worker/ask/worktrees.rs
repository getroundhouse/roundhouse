//! What a worker asks its hub about a workspace's other worktrees, the ones made for its work besides its own, which
//! `docs/spec/workspaces.md` describes: making one, removing one, and listing them all.

use serde::{Deserialize, Serialize};

use super::{Answer, Ask, AskReply, WorkerAsk};
use crate::model::{AskKey, BranchName, ChildWorktree, CommitId, GitStatus, WorkName};

/// Another worktree for the workspace's work, which the hub makes and records, so it goes when the workspace is
/// archived. A name that's taken gets `-2`, `-3` and so on, so asking never needs a second try. Asked again with the
/// same `key`, it answers with what it made the first time.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CreateWorktree {
    pub on: NewWorktreeOn,
    pub key: AskKey,
}

impl Ask for CreateWorktree {
    type Answer = WorktreeCreated;

    fn into_ask(self) -> WorkerAsk {
        WorkerAsk::CreateWorktree(self)
    }
}

/// What a new worktree checks out. A hub that doesn't know a kind can't read the ask, and refuses it, rather than make
/// the worktree some other way.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NewWorktreeOn {
    /// A new branch in the workspace's folder, named for the work, as `euston/try-the-parser`, from `base`, a branch,
    /// tag or commit, or from the commit the workspace's worktree has checked out.
    NewBranch {
        name: WorkName,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<String>,
    },
    /// One of the workspace's branches as it is, such as one a worktree that's been removed was on.
    Existing { branch: BranchName },
}

/// The worktree made, as the hub recorded it, and the commit it has checked out.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorktreeCreated {
    pub worktree: ChildWorktree,
    pub commit: CommitId,
    /// The ignored files copied into it, as setup copies them, such as `.env`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub copied: Vec<String>,
    /// What went wrong copying them, which didn't stop it being made.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl Answer for WorktreeCreated {
    fn read(reply: AskReply) -> Result<Self, AskReply> {
        let AskReply::WorktreeCreated(created) = reply else { return Err(reply) };
        Ok(created)
    }
}

/// Removes one of the workspace's other worktrees, keeping its branch and its record. Not removed if it has changes
/// that aren't committed or commits on no branch, unless `force`. Refused with `Conflict` for the workspace's own
/// worktree, and `NotFound` for any other that isn't the workspace's.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RemoveWorktree {
    pub path: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub force: bool,
}

impl Ask for RemoveWorktree {
    type Answer = WorktreeRemoval;

    fn into_ask(self) -> WorkerAsk {
        WorkerAsk::RemoveWorktree(self)
    }
}

/// What a [`RemoveWorktree`] is answered with: either of two kinds of answer, since not removing one for what it
/// would lose says what that is, for the agent that asked to read, where an error has only a sentence.
#[derive(Clone, Debug, PartialEq)]
#[expect(
    clippy::large_enum_variant,
    reason = "an answer is moved once, as it's read, so boxing the larger kind would only add an allocation"
)]
pub enum WorktreeRemoval {
    Removed(WorktreeRemoved),
    WouldLose(WouldLose),
}

impl Answer for WorktreeRemoval {
    fn read(reply: AskReply) -> Result<Self, AskReply> {
        match reply {
            AskReply::WorktreeRemoved(removed) => Ok(Self::Removed(removed)),
            AskReply::WouldLose(lose) => Ok(Self::WouldLose(lose)),
            AskReply::PullRequests(_)
            | AskReply::PullRequest(_)
            | AskReply::Wait(_)
            | AskReply::WorktreeCreated(_)
            | AskReply::Worktrees(_) => Err(reply),
        }
    }
}

impl From<WorktreeRemoval> for AskReply {
    fn from(removal: WorktreeRemoval) -> Self {
        match removal {
            WorktreeRemoval::Removed(removed) => Self::WorktreeRemoved(removed),
            WorktreeRemoval::WouldLose(lose) => Self::WouldLose(lose),
        }
    }
}

/// The worktree removed, as the hub recorded it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorktreeRemoved {
    pub worktree: ChildWorktree,
    /// What its git said just before. `None` when it had gone already, or git couldn't say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<GitStatus>,
}

/// Not removed, since that would lose what `status` says isn't committed or is on no branch. Asking with `force`
/// removes it anyway.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WouldLose {
    pub status: GitStatus,
}

/// The workspace's worktrees, its own and each other one that's there or being made, with what their git says now.
#[derive(Clone, Copy, Debug)]
pub struct ListWorktrees;

impl Ask for ListWorktrees {
    type Answer = Worktrees;

    fn into_ask(self) -> WorkerAsk {
        WorkerAsk::ListWorktrees
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Worktrees {
    pub own: OwnWorktree,
    /// Oldest first.
    pub children: Vec<ListedChild>,
}

/// The workspace's own worktree, with what its git says now.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OwnWorktree {
    pub path: String,
    /// `None` when git couldn't say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<GitStatus>,
}

/// One of the workspace's other worktrees, with what its git says now.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ListedChild {
    pub worktree: ChildWorktree,
    /// `None` when git couldn't say, as when the worktree has gone without the hub knowing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<GitStatus>,
}

impl Answer for Worktrees {
    fn read(reply: AskReply) -> Result<Self, AskReply> {
        let AskReply::Worktrees(worktrees) = reply else { return Err(reply) };
        Ok(worktrees)
    }
}
