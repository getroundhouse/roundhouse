//! The requests for a session's agent: a prompt sent now, the answer to what it asked, cancelling its turn, and
//! compacting its conversation. A prompt queued for later is in [`super::queue`].

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::event::ApprovalDecision;
use crate::model::{ApprovalId, AttachmentId, ClientMessageId, Seq, SessionId};
use crate::prompt::Prompt;

/// Resolves once the prompt is recorded. Sending the same `client_msg_id` twice records it once, so retrying after a
/// disconnect can't send it twice. A retry only records: if the hub stopped before delivering the original, the session
/// shows as interrupted, and the prompt needs sending again with a new ID. Each of the prompt's files has to have been
/// added to this session first, and a prompt with any can have no text.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionPrompt {
    pub id: SessionId,
    pub client_msg_id: Option<ClientMessageId>,
    #[serde(flatten)]
    pub prompt: Prompt<AttachmentId>,
}

impl Request for SessionPrompt {
    type Reply = PromptRecorded;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionPrompt(self)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PromptRecorded {
    /// Where it's recorded.
    pub seq: Seq,
    /// Whether it had been recorded already, under the same `client_msg_id`.
    pub duplicate: bool,
}

impl Reply for PromptRecorded {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::PromptRecorded(recorded) = response else { return Err(response) };
        Ok(recorded)
    }
}

/// Answering again is refused, changing nothing.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionApprove {
    pub id: SessionId,
    pub approval_id: ApprovalId,
    pub decision: ApprovalDecision,
}

impl Request for SessionApprove {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionApprove(self)
    }
}

/// Cancelling when nothing is running does nothing. Sent again once a new turn has started, it cancels that one too,
/// since it carries no ID.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionCancel {
    pub id: SessionId,
}

impl Request for SessionCancel {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionCancel(self)
    }
}

/// Asks the agent to compact its conversation now, keeping what `keeping` says if it says anything. Refused with
/// `Conflict` while the agent is on a turn, or has a prompt it hasn't taken, since it would only compact once that's
/// done, and with `InvalidRequest` for an agent whose description says it doesn't compact when asked. Recorded as a
/// `CompactionAsked`, so it's sent once.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionCompact {
    pub id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keeping: Option<String>,
}

impl Request for SessionCompact {
    type Reply = ();

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionCompact(self)
    }
}
