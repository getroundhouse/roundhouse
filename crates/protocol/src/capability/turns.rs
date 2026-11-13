//! Turns: cancelling one, what its end says, the ones the agent starts by itself, and the kinds of turn there are.

use serde::{Deserialize, Serialize};

use super::Claimed;
use super::area::area;
use super::calls::call::LineRange;
use crate::model::{BranchName, CommitId};

area! { "a turn";
    /// A turn can be cancelled. The agent stops what it's running, drops each prompt it hadn't taken, resolves each
    /// open request as cancelled, ends the turn once, as cancelled, and takes the next prompt, in the same process
    /// where it can. What becomes of the work it left running in the background is [`Cancel::stops`], and the adapter
    /// reports each task the cancel stopped as stopped. Every agent takes a prompt as a turn and ends it, so neither
    /// is a capability.
    Cancel => cancel: Cancel,
    /// A turn's end says why it ended, as an [`Ending`], so a client tells a turn that ran out of context from one the
    /// person cancelled without reading the agent's words.
    Endings => endings: Claimed,
    /// A turn's end says how many tokens its own requests took, its subagents' left out.
    Tokens => tokens: Claimed,
    /// A turn's end says what the conversation has cost so far, in US dollars, across every launch, as the agent works
    /// it out from its model's prices, as Claude's `total_cost_usd` does. A turn's own cost is what it adds to the
    /// total before it, which the hub works out.
    Cost => cost: Claimed,
    /// The agent starts turns by itself, with no prompt, and says why, as an [`OwnTurn`], which the hub records. The
    /// hub records each like any turn, and doesn't take it for one a prompt started.
    OwnTurns => own_turns: Claimed,
    /// A chat can have a goal the agent works toward across turns, starting turns by itself until it's met, blocked,
    /// paused or out of budget, and the goal is reported whenever it changes, which the hub keeps on the session's
    /// record. The person sets, pauses and clears it. The agent can propose one, which the person accepts by setting
    /// it.
    Goals => goals: Claimed,
    /// A turn can review changes rather than make them: those not committed, a branch's against another, one commit,
    /// or what the person describes. The hub records the request before it's sent, and the agent takes or drops it,
    /// as it does a prompt. The review is the turn's reply.
    Reviews => reviews: Claimed,
    /// A review's findings are reported beside its reply, each with where it is and how much it matters, which the hub
    /// records, as Claude's `ReportFindings` gives them. Codex's app server gives only the reply.
    ReviewFindings => review_findings: Claimed,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cancel {
    pub stops: CancelStops,
}

/// What a cancel stops besides the turn, which a client says before the person cancels.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CancelStops {
    /// Nothing: background tasks carry on, as Claude's do, which its adapter stops one at a time when asked.
    Nothing,
    /// Every background task too, subagents' included, as opencode's abort does.
    Tasks,
    #[serde(other)]
    Unknown,
}

/// Why a turn ended.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Ending {
    /// The agent finished.
    Done,
    /// Someone cancelled it.
    Cancelled,
    /// The person refused a use, and the refusal stopped the turn.
    Refused,
    /// One of the agent's requests to its model failed, as the failure before it says.
    RequestFailed,
    /// The conversation no longer fits the model's context, and compacting didn't make room.
    ContextFull,
    /// The model's reply reached the most it can write.
    ReplyTooLong,
    /// The model's provider blocked the reply.
    Blocked,
    /// It reached the most requests the agent makes in a turn.
    StepLimit,
    /// It spent the chat's budget.
    OverBudget,
    /// One of the agent's hooks stopped it.
    Hook,
    /// The reply didn't match the schema the prompt asked for.
    NotStructured,
    #[serde(other)]
    Unknown,
}

/// Why the agent started a turn by itself.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OwnTurn {
    /// One of its background tasks ended, and it takes in how.
    TaskEnded,
    /// It set itself to carry on at a time, which came.
    Scheduled,
    /// Its goal isn't met.
    Goal,
    /// One of its subagents told it something.
    SubagentMessage,
    /// It compacted mid-turn, and carries on with what it was doing.
    AfterCompaction,
    /// One of its tool servers failed, and it takes that in.
    ServerFailed,
    #[serde(other)]
    Unknown,
}

/// A chat's goal, as [`Capability::Goals`] reports it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Goal {
    /// What's to be done, or the condition that says it's done.
    pub objective: String,
    pub status: GoalStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u64>,
    #[serde(default)]
    pub used_tokens: u64,
    /// How many turns it has taken so far.
    #[serde(default)]
    pub turns: u32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    /// The agent proposed it, and it waits for the person.
    Proposed,
    Active,
    Paused,
    /// The agent can't go on without the person.
    Blocked,
    /// The account's limits stopped it until they start again.
    Limited,
    OverBudget,
    Met,
    #[serde(other)]
    Unknown,
}

/// What a review looks at.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "reviews", rename_all = "snake_case")]
pub enum ReviewTarget {
    Uncommitted,
    Branch { against: BranchName },
    Commit { commit: CommitId },
    Described { instructions: String },
}

/// One thing a review found.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub title: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<LineRange>,
    /// From 0, which matters most, as the agent ranks them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence_percent: Option<u8>,
}
