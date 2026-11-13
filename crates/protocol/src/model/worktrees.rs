//! The other worktrees an agent makes for a workspace's work, besides the workspace's own, which
//! `docs/spec/workspaces.md` describes. The hub records each, so it goes when the workspace is archived, rather than
//! lasting until an agent remembers to remove it.

use serde::{Deserialize, Serialize};

use super::{BranchName, Branches, ChildWorktreeId, Rev, SessionId, ToolUseId, WorkspaceId};

/// Another worktree of a workspace's repository, made for its work, in [`CHILDREN`] beside the workspaces, in a folder
/// named after its workspace. No workspace's name starts with a dot, so `.children` is no workspace's. A child isn't
/// inside the workspace's own worktree, where it would show in `git status` and go along when archiving moves the
/// worktree aside.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChildWorktree {
    pub id: ChildWorktreeId,
    pub workspace_id: WorkspaceId,
    /// On the worker's machine. No other of the workspace's worktrees that's there has it, though one removed may.
    pub path: String,
    /// The branch it was made on, which it may have left since, as `branches` says.
    pub branch: BranchName,
    /// The chat whose agents made it.
    pub session_id: SessionId,
    pub made_by: MadeBy,
    pub created_at_ms: i64,
    pub status: ChildStatus,
    /// Its branches, as its worker last read them. `None` until a worker has watched them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branches: Option<Branches>,
    #[serde(default)]
    pub rev: Rev,
}

/// Which of a chat's agents made a worktree.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MadeBy {
    /// The chat's own agent.
    Agent,
    /// One of its subagents, by the call that started it.
    Subagent { call: ToolUseId },
    /// One of the chat's agents, but the hub couldn't tell which.
    Undetermined,
    /// From a newer hub.
    #[serde(other)]
    Unknown,
}

/// Where a worktree is in its life.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChildStatus {
    /// Recorded before its worker is asked to make it, so a hub that stops meanwhile leaves a record of what may be
    /// there, rather than a worktree nothing knows of.
    Creating,
    Ready,
    /// Removed, by an agent or with the workspace. Its record stays, with its branch, so a worktree can be made on
    /// that branch again.
    Removed {
        removed_at_ms: i64,
    },
    /// From a newer hub.
    #[serde(other)]
    Unknown,
}

/// Where the workspaces' other worktrees go, beside the workspaces of their repository.
pub const CHILDREN: &str = ".children";

/// What an agent calls a piece of work, such as `try-the-parser`, which names its branch in the workspace's folder,
/// as `holly/try-the-parser`, and the directory of a worktree made for it. Lowercase letters, digits and single
/// hyphens, since it names a directory on file systems that don't tell case apart as well as a branch, and at most
/// [`WorkName::MOST_CHARS`] of them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct WorkName(String);

/// Why a name can't name a piece of work.
#[derive(thiserror::Error, Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkNameError {
    #[error("a name for the work can't be empty")]
    Empty,
    #[error("a name for the work is at most {} characters", WorkName::MOST_CHARS)]
    TooLong,
    #[error("a name for the work is lowercase letters, digits and single hyphens, such as `try-the-parser`")]
    NotKebab,
    /// Windows can't name a directory this.
    #[error("a name for the work can't be one Windows keeps for a device, such as `con` or `com1`")]
    Reserved,
}

/// The names Windows keeps for devices, which no directory there can have.
const RESERVED: [&str; 4] = ["con", "prn", "aux", "nul"];
/// The same, each followed by a digit, as `com1`.
const RESERVED_WITH_A_DIGIT: [&str; 2] = ["com", "lpt"];

impl WorkName {
    /// Short enough that a branch's name, after its folder and any number a taken name gets, still fits on a line.
    pub const MOST_CHARS: usize = 40;

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for WorkName {
    type Error = WorkNameError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        if name.is_empty() {
            return Err(WorkNameError::Empty);
        }
        let kebab = name.split('-').all(|word| {
            !word.is_empty() && word.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        });
        if !kebab {
            return Err(WorkNameError::NotKebab);
        }
        // Every character is one byte by now, so this counts characters, as the limit says.
        if name.len() > Self::MOST_CHARS {
            return Err(WorkNameError::TooLong);
        }
        let with_a_digit = name.len() == 4 && name.as_bytes()[3].is_ascii_digit();
        if RESERVED.contains(&name.as_str()) || (with_a_digit && RESERVED_WITH_A_DIGIT.contains(&&name[..3])) {
            return Err(WorkNameError::Reserved);
        }
        Ok(Self(name))
    }
}

impl From<WorkName> for String {
    fn from(name: WorkName) -> String {
        name.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_lowercase_words_joined_by_single_hyphens() {
        let name = |text: &str| WorkName::try_from(text.to_string()).map(String::from);

        assert_eq!(name("try-the-parser"), Ok("try-the-parser".to_string()));
        assert_eq!(name("v2"), Ok("v2".to_string()));
        assert_eq!(name(""), Err(WorkNameError::Empty));
        assert!(name(&"a".repeat(WorkName::MOST_CHARS)).is_ok(), "the longest");
        assert_eq!(name(&"a".repeat(WorkName::MOST_CHARS + 1)), Err(WorkNameError::TooLong));
        let long_in_bytes_only = "é".repeat(WorkName::MOST_CHARS / 2 + 1);
        for refused in
            ["Try", "try--parser", "-try", "try-", "try_parser", "try/parser", "../up", "trý", &long_in_bytes_only]
        {
            assert_eq!(name(refused), Err(WorkNameError::NotKebab), "{refused}");
        }
        for reserved in ["con", "prn", "aux", "nul", "com1", "lpt9"] {
            assert_eq!(name(reserved), Err(WorkNameError::Reserved), "{reserved}");
        }
        for allowed in ["console", "com10", "come", "comp", "lpta"] {
            assert!(name(allowed).is_ok(), "{allowed}");
        }
    }

    #[test]
    fn a_name_that_isnt_one_doesnt_read() {
        assert!(serde_json::from_str::<WorkName>(r#""Try The Parser""#).is_err());
        assert_eq!(serde_json::from_str::<WorkName>(r#""try""#).unwrap().as_str(), "try");
    }
}
