//! The rules an agent goes by, or suggests keeping, and where each is kept.

use serde::{Deserialize, Serialize};

use crate::capability::calls::CallKind;

/// Where a rule is kept, which says who else it holds for.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Kept {
    /// For this conversation only.
    Conversation,
    /// For this worktree, in a file git doesn't track.
    Worktree,
    /// For everyone working on the project, in a file the repository tracks, which shows in the workspace's changes.
    Project,
    /// For everything the person runs on the agent's machine.
    User,
    #[serde(other)]
    Unknown,
}

/// A rule the agent goes by, or suggests keeping.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    #[serde(flatten)]
    pub covers: Covers,
    /// What it matches, in the agent's own form, such as Claude's `npm test:*`, a command's first words for Codex, or
    /// opencode's wildcards. A client shows it and sends it back, and only the adapter reads it.
    pub matching: String,
    pub effect: RuleEffect,
    pub kept: Kept,
    /// What it means, in words, when the agent says, such as "Allow npm test commands".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub described: Option<String>,
}

impl Rule {
    /// Whether it holds nothing this build doesn't know, so an adapter can tell its agent to keep it.
    pub fn is_known(&self) -> bool {
        let Rule { covers, matching: _, effect, kept, described: _ } = self;
        let covers = match covers {
            Covers::Calls { call } => *call != CallKind::Unknown,
            Covers::Network => true,
            Covers::Unknown => false,
        };
        covers && *effect != RuleEffect::Unknown && *kept != Kept::Unknown
    }
}

/// What a rule is for.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(tag = "covers", rename_all = "snake_case")]
pub enum Covers {
    /// Calls of one kind, such as commands.
    Calls { call: CallKind },
    /// Reaching hosts on the network, as [`Capability::NetworkRequests`](super::Capability::NetworkRequests) says.
    Network,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuleEffect {
    Allow,
    Ask,
    Deny,
    #[serde(other)]
    Unknown,
}
