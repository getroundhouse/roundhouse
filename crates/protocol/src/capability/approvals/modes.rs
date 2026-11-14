//! Approval modes, as the parts every agent's modes are made of.

use serde::{Deserialize, Serialize};

/// The modes an agent offers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Modes {
    /// The one a chat starts in.
    pub default: ApprovalMode,
    /// The others a chat can choose.
    #[serde(default)]
    pub others: Vec<ApprovalMode>,
}

impl Modes {
    /// Whether `mode` is one of them. One that holds anything this build doesn't know is none, since it would match
    /// another unknown one without being the same mode.
    pub fn offers(&self, mode: ApprovalMode) -> bool {
        mode.is_known() && (self.default == mode || self.others.contains(&mode))
    }
}

/// How much an agent does without asking, what becomes of a use beyond that, what the OS keeps it to, and whether it
/// plans first. Each of Claude's modes is one, such as `acceptEdits`, which does edits unasked and asks before the
/// rest, and so is each of Codex's combinations of sandbox, approval policy and reviewer, such as `workspace-write`
/// with `never`. A kind offers the ones its agent has, and makes others from what its agent does have where it can,
/// such as opencode's from rules.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ApprovalMode {
    pub unasked: Unasked,
    pub beyond: Beyond,
    /// What the OS keeps the agent's commands to, whatever they're allowed, or `None` where nothing does. A kind
    /// offers none unless its agent's commands run in a sandbox the OS enforces, since the agent's own checks of a
    /// command can be got around.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<Sandbox>,
    /// It works out a plan and changes nothing until the plan is approved, which [`super::super::asking`] says how it
    /// asks.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plan: bool,
}

impl ApprovalMode {
    /// Whether it holds nothing this build doesn't know, so an adapter can tell its agent.
    pub fn is_known(self) -> bool {
        let ApprovalMode { unasked, beyond, sandbox, plan: _ } = self;
        unasked != Unasked::Unknown && beyond != Beyond::Unknown && sandbox != Some(Sandbox::Unknown)
    }
}

/// What an agent does without asking.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Unasked {
    /// What only reads, such as reading files and looking through them, as in Claude's `default` and Codex's
    /// `untrusted`.
    Reads,
    /// What reads, and edits to files, as in Claude's `acceptEdits`. A command is beyond it.
    Edits,
    /// Everything, within its sandbox where it has one, as in Codex's `on-request` with one and Claude's
    /// `bypassPermissions` without. Without a sandbox, that's full access, but for what [`AlwaysAsks`] covers.
    ///
    /// [`AlwaysAsks`]: super::Capability::AlwaysAsks
    Everything,
    #[serde(other)]
    Unknown,
}

/// What becomes of a use beyond what the agent does without asking.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Beyond {
    /// The agent asks the person, as [`Requests`](super::Capability::Requests) says.
    Asks,
    /// The agent refuses it without asking, as Claude's `dontAsk` and Codex's `never` do.
    Refused,
    /// A reviewer the agent runs decides in the person's place, as
    /// [`ReviewVerdicts`](super::Capability::ReviewVerdicts) says.
    Reviewer,
    #[serde(other)]
    Unknown,
}

/// What the OS keeps an agent's commands to.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Sandbox {
    /// Nothing can be written.
    ReadOnly,
    /// Only the worktree and the temporary directory can be written.
    Worktree,
    #[serde(other)]
    Unknown,
}
