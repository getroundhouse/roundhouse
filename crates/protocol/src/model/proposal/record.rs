use std::fmt;

use serde::{Deserialize, Serialize};

use super::{Proposed, Response, RevisionSummary};
use crate::event::LinkedImage;
use crate::model::{ProposalId, Rev, SessionId, ToolUseId, WorkspaceId};

/// A proposal as the hub keeps it, in the chat its agent wrote it in, with its newest revision. Each revision is kept
/// as it was written, with the person's response to it, if any, and only the newest can be answered. A list of
/// proposals carries only the newest of each, so it stays as small however often one is revised, and
/// [`ProposalRevisionsRead`](crate::client::proposals::ProposalRevisionsRead) reads the rest, a page at a time.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub id: ProposalId,
    pub workspace_id: WorkspaceId,
    pub session_id: SessionId,
    /// The newest revision, whose number says how many there are.
    pub current: ProposalRevision,
    /// Each revision in short, oldest first, the newest too, so a chat draws each revision's line, and the timeline
    /// each revision, from the record. A hub from before revisions sends none, for a proposal with only its first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revisions: Vec<RevisionSummary>,
    /// Once the agent withdrew it, as no longer of use, why and when, so nothing more can be said to it. A hub from
    /// before withdrawing sends none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub withdrawn: Option<Withdrawal>,
    pub created_at_ms: i64,
    #[serde(default)]
    pub rev: Rev,
}

/// The agent's withdrawal of a proposal it made, once it was obsolete or wouldn't be used, such as when the person
/// decided another way in the chat.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Withdrawal {
    /// Why, in a sentence, which the person reads in the proposal's tab.
    pub why: String,
    /// The agent's call that withdrew it, which the chat draws its line for, as [`ProposalRevision::call`] says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<ToolUseId>,
    pub at_ms: i64,
}

/// One revision of a proposal: what the agent wrote, and the person's response to it, once they've given one.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProposalRevision {
    pub number: RevisionNumber,
    /// What changed since the revision before, as the agent said when it revised, in Markdown. Empty for the first.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub changed: String,
    /// The agent's call that wrote it, which the chat draws its line for. `None` from an agent's CLI that doesn't say
    /// which call a tool's use is, whose chat draws the call as any other.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<ToolUseId>,
    pub proposed: Proposed,
    pub made_at_ms: i64,
    /// The person's response, once they've given it. `None` while it waits for them, and for an earlier revision that
    /// was revised before they responded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<Response>,
    /// The images what the agent wrote points to in the worktree, as they were when it wrote it, each by the address it
    /// gives it, as a reply's text names them. The hub keeps each as an attachment of the chat, so a client reads its
    /// bytes with `AttachmentRead`. One that wasn't there, or wasn't an image inside the worktree, isn't named.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<LinkedImage>,
}

/// Which revision of its proposal one is: 1 for the first the agent wrote, and one more for each after it. An answer
/// names the one the person read, so the hub refuses it once the agent has written another.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct RevisionNumber(pub u32);

impl RevisionNumber {
    pub const FIRST: Self = Self(1);

    /// The revision after this one.
    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl Proposal {
    /// The revision `number`, in short, if it has one.
    pub fn revision(&self, number: RevisionNumber) -> Option<&RevisionSummary> {
        self.revisions.iter().find(|revision| revision.number == number)
    }

    /// Whether it's been revised, so each revision is named by its number.
    pub fn revised(&self) -> bool {
        self.current.number > RevisionNumber::FIRST
    }
}

impl fmt::Display for RevisionNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
