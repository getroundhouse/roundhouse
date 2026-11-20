//! The requests that answer one of an agent's proposals, or ask for another round of it, and read its earlier
//! revisions.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{ClientRequestId, OptionPlace, Proposal, ProposalId, ProposalRevision, RevisionNumber};

/// Chooses one option for each of the decisions of the proposal's revision `revision`, with a note, which the hub
/// keeps on that revision, and tells its agent what was chosen in a prompt of its own, which the agent reads the rest
/// of with its tools. It's refused as an invalid request when
/// [`Proposed::check_answer`](crate::model::Proposed::check_answer) refuses it, and as a conflict once the proposal's
/// been answered otherwise, or when `revision` isn't its newest, so an answer is always to what the person read.
/// Sending the same `client_request_id` again answers with the proposal as it was answered, and tells the agent once.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProposalAnswer {
    pub id: ProposalId,
    pub revision: RevisionNumber,
    pub chosen: Vec<OptionPlace>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    pub client_request_id: ClientRequestId,
}

impl Request for ProposalAnswer {
    type Reply = Proposal;

    fn into_request(self) -> ClientRequest {
        ClientRequest::ProposalAnswer(self)
    }
}

/// Asks the agent for another revision of the proposal's revision `revision`, without choosing, with a note that says
/// what it should change, which the hub keeps on that revision as the person's response, and tells the agent of in a
/// prompt of its own, as [`ProposalAnswer`] does. It's refused as [`ProposalAnswer`] is, and as an invalid request
/// without a note.
///
/// It's a request of its own, which a hub older than this refuses as unsupported. As a field of [`ProposalAnswer`], an
/// older hub would read it as an answer that chose nothing. *Unconfirmed.*
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProposalAnotherRound {
    pub id: ProposalId,
    pub revision: RevisionNumber,
    pub note: String,
    pub client_request_id: ClientRequestId,
}

impl Request for ProposalAnotherRound {
    type Reply = Proposal;

    fn into_request(self) -> ClientRequest {
        ClientRequest::ProposalAnotherRound(self)
    }
}

impl Reply for Proposal {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::Proposal(proposal) = response else { return Err(response) };
        Ok(proposal)
    }
}

/// The revisions of the proposal `id` from `from` on, oldest first, at most `limit` of them, and at least one when
/// there are any from `from`, but never more than [`ProposalRevisionsRead::MOST`], so however often a proposal is
/// revised, a read is no larger than a few of its revisions. A proposal lists only its newest, whose number says how
/// many there are. One the hub doesn't have is refused as not found, and a hub older than this refuses it as
/// unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProposalRevisionsRead {
    pub id: ProposalId,
    pub from: RevisionNumber,
    pub limit: u32,
}

impl ProposalRevisionsRead {
    /// The most revisions a read sends, whatever it asks for: each can be as large as
    /// [`MAX_PROPOSAL_BYTES`](crate::model::MAX_PROPOSAL_BYTES).
    pub const MOST: u32 = 8;
}

impl Request for ProposalRevisionsRead {
    type Reply = ProposalRevisions;

    fn into_request(self) -> ClientRequest {
        ClientRequest::ProposalRevisionsRead(self)
    }
}

/// Some of a proposal's revisions, oldest first.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProposalRevisions {
    pub revisions: Vec<ProposalRevision>,
    /// The number of its newest revision when they were read, so a reader knows how far they go.
    pub newest: RevisionNumber,
}

impl Reply for ProposalRevisions {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::ProposalRevisions(revisions) = response else { return Err(response) };
        Ok(revisions)
    }
}
