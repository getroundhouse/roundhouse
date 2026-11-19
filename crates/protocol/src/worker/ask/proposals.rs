//! What a worker asks its hub to propose for its agent, to revise a proposal or withdraw it, and to read one back.

use serde::{Deserialize, Serialize};

use super::{Answer, Ask, AskReply, WorkerAsk};
use crate::attachment::NewAttachment;
use crate::event::LinkedImage;
use crate::model::{AskKey, Proposal, ProposalId, ProposalRevision, Proposed, RevisionNumber, ToolUseId};

/// Makes a proposal in the agent's chat, which a hub refuses, saying why, when [`Proposed::check`] does. Asked again
/// with the same `key`, as a new hub is asked whatever the last didn't answer, it answers with the one it made.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Propose {
    pub proposed: Proposed,
    /// The agent's call that asked, when its CLI says, which the chat draws the proposal's line for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<ToolUseId>,
    pub key: AskKey,
    /// The images what the agent wrote points to in the worktree, each by the address it gives it, as a reply's text
    /// names them, which the hub keeps with the revision. A hub from before images ignores them, and the page draws
    /// none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<LinkedImage>,
    /// The bytes of each of `images`, as the worker read them, which the hub keeps as the chat's, as it does a reply's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<NewAttachment>,
}

impl Ask for Propose {
    type Answer = ProposalRecord;

    fn into_ask(self) -> WorkerAsk {
        WorkerAsk::Propose(self)
    }
}

/// Writes a new revision of one of the chat's proposals, whether or not the person has answered the newest, which a hub
/// refuses, saying why, when [`Proposed::check`] does, and as not found for another chat's. Asked again with the same
/// `key`, it answers with the proposal as it is now, and writes nothing more.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Revise {
    pub id: ProposalId,
    pub proposed: Proposed,
    /// What changed since the revision before, which the new revision shows first.
    pub changed: String,
    /// As [`Propose::call`] says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<ToolUseId>,
    pub key: AskKey,
    /// As [`Propose::images`] says.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<LinkedImage>,
    /// As [`Propose::attachments`] says.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<NewAttachment>,
}

impl Ask for Revise {
    type Answer = ProposalRecord;

    fn into_ask(self) -> WorkerAsk {
        WorkerAsk::Revise(self)
    }
}

/// Withdraws one of the chat's proposals, once it's obsolete or won't be used, so it no longer waits for the person,
/// which a hub refuses without `why`, as a conflict once the person has chosen, and as not found for another chat's.
/// Asked again, as a new hub is asked whatever the last didn't answer, it answers with the proposal as it was withdrawn.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Withdraw {
    pub id: ProposalId,
    /// Why, in a sentence, which the person reads in the proposal's tab.
    pub why: String,
    /// As [`Propose::call`] says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<ToolUseId>,
}

impl Ask for Withdraw {
    type Answer = ProposalRecord;

    fn into_ask(self) -> WorkerAsk {
        WorkerAsk::Withdraw(self)
    }
}

/// One of the chat's proposals, as the hub keeps it, and its revision `revision` in full, when it's asked for one
/// that isn't the newest. Another chat's is refused as not found, and so is a revision it doesn't have.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReadProposal {
    pub id: ProposalId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<RevisionNumber>,
}

impl Ask for ReadProposal {
    type Answer = ProposalRecord;

    fn into_ask(self) -> WorkerAsk {
        WorkerAsk::ReadProposal(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProposalRecord {
    pub proposal: Proposal,
    /// The revision [`ReadProposal`] asked for, in full, when it isn't the newest, which `proposal` has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<Box<ProposalRevision>>,
}

impl Answer for ProposalRecord {
    fn read(reply: AskReply) -> Result<Self, AskReply> {
        let AskReply::Proposal(answer) = reply else { return Err(reply) };
        Ok(answer)
    }
}
