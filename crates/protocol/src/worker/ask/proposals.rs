//! What a worker asks its hub to propose for its agent, and to read a proposal back.

use serde::{Deserialize, Serialize};

use super::{Answer, Ask, AskReply, WorkerAsk};
use crate::attachment::NewAttachment;
use crate::event::LinkedImage;
use crate::model::{AskKey, Proposal, ProposalId, Proposed, ToolUseId};

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

/// One of the chat's proposals, as the hub keeps it. Another chat's is refused as not found.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReadProposal {
    pub id: ProposalId,
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
}

impl Answer for ProposalRecord {
    fn read(reply: AskReply) -> Result<Self, AskReply> {
        let AskReply::Proposal(answer) = reply else { return Err(reply) };
        Ok(answer)
    }
}
