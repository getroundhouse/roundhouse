//! The person's answer to a proposal, or their request for another round of it, which the hub keeps on it and tells
//! the agent of in a prompt of its own. The prompt says only what was chosen, or that another round is asked for, and
//! the agent reads the rest, such as the note, with `get_proposal`. The prompt
//! goes in the chat's queue in the transaction that keeps the answer, so the agent is told of every answer kept, even
//! after the hub restarts, and is then sent at once, into the agent's turn while it works, which steers it, unless
//! the queue waits for the person.

use std::sync::Arc;

use super::super::{Hub, new_id};
use crate::store::{AnswerToKeep, Answering, now_ms};
use protocol::client::proposals::{ProposalAnotherRound, ProposalAnswer};
use protocol::client::{ApiError, HubNotice};
use protocol::model::{
    AnswerProblem, Choice, ClientMessageId, ClientRequestId, Proposal, ProposalId, ProposalRevision, Response,
    RevisionNumber, Round,
};
use protocol::prompt::Prompt;
use tracing::warn;

impl Hub {
    /// Keeps the answer, once it's checked, with the prompt that tells the agent, as [`Hub::respond`] says.
    pub(crate) async fn answer_proposal(self: &Arc<Self>, answer: ProposalAnswer) -> Result<Proposal, ApiError> {
        let ProposalAnswer { id, revision, chosen, note, client_request_id } = answer;
        let choice = Choice { chosen, note: note.trim().to_string(), at_ms: now_ms() };
        self.respond(id, revision, Response::Chose(choice), client_request_id).await
    }

    /// Keeps the request for another round, once it's checked, with the prompt that tells the agent, as
    /// [`Hub::respond`] says.
    pub(crate) async fn ask_another_round(self: &Arc<Self>, round: ProposalAnotherRound) -> Result<Proposal, ApiError> {
        let ProposalAnotherRound { id, revision, note, client_request_id } = round;
        let round = Round { note: note.trim().to_string(), at_ms: now_ms() };
        self.respond(id, revision, Response::AnotherRound(round), client_request_id).await
    }

    /// Keeps `response` to the revision `revision` of the proposal `id`, once it's checked, with the prompt that tells
    /// the agent, which the hub writes and gives an ID of its own. The same request sent again answers with the
    /// proposal as it was answered, and tells the agent nothing more. One answered by another request, or revised
    /// since the revision answered, is refused as a conflict.
    async fn respond(
        self: &Arc<Self>,
        id: ProposalId,
        revision: RevisionNumber,
        response: Response,
        request: ClientRequestId,
    ) -> Result<Proposal, ApiError> {
        let lookup = id.clone();
        let proposal = self.blocking(move |store| store.get_proposal(&lookup)).await?;
        // The store checks it, against the revision as it keeps it, with whether it's answered already or revised since.
        let session = self.live_session(&proposal.session_id).await?;
        let message = ClientMessageId::from(new_id());
        let queued_as = message.clone();
        let (answering, _) = session
            .queue_with(move |store| {
                let told = |proposal: &Proposal, response: &Response| Prompt::new(told(proposal, response));
                let keep = AnswerToKeep { revision, response: &response, request: &request, message, told: &told };
                // Whether it's newly answered, or refused.
                Ok(match store.answer_proposal(&id, keep)? {
                    Answering::Kept { proposal, queued } => (Ok((proposal, true)), queued.queuing()),
                    Answering::Again(proposal) => (Ok((proposal, false)), None),
                    Answering::Otherwise(_) => (Err(AnswerProblem::Answered), None),
                    Answering::Revised(proposal) => {
                        (Err(AnswerProblem::Revised { newest: proposal.current.number }), None)
                    }
                    Answering::Refused(problem) => (Err(problem), None),
                })
            })
            .await?;
        let (proposal, new) = answering?;
        if new {
            self.notify(HubNotice::Proposal(proposal.clone()));
            // The agent's to have it at its next chance, steering it as the person's own prompt would, and only a held
            // queue keeps it there. It's queued first, with the answer, so it's sent even if this hub goes away now.
            if let Err(e) = session.send_now_unless_held(queued_as).await {
                warn!(session = %proposal.session_id, "couldn't send the answer to a proposal now: {}", e.message);
            }
        }
        Ok(proposal)
    }
}

/// What the agent is told: what was chosen, in which proposal, and that it reads the note with its tool, or that the
/// person asks for another round, and reads what to change in the note.
pub(super) fn told(proposal: &Proposal, response: &Response) -> String {
    let title = proposal.current.proposed.title.trim();
    // Once there are several, the revision the person read, which may not be the newest when the agent reads this.
    let which = if proposal.revised() {
        format!("revision {} of your proposal \"{title}\" ({})", proposal.current.number, proposal.id)
    } else {
        format!("your proposal \"{title}\" ({})", proposal.id)
    };
    let reads = if proposal.revised() {
        format!("get_proposal reads, given revision {}", proposal.current.number)
    } else {
        "get_proposal reads".to_string()
    };
    let choice = match response {
        Response::Chose(choice) => choice,
        Response::AnotherRound(_) => {
            return format!(
                "I'd like another round of {which}, without choosing yet. My note says what to change, which {reads}. \
                 Then revise it with revise_proposal."
            );
        }
        Response::Unknown => return format!("I responded to {which}, which {reads}."),
    };
    let answered_revision = ProposalRevision { response: Some(response.clone()), ..proposal.current.clone() };
    let chosen = answered_revision.chosen_in_words().unwrap_or_default();
    let said = match proposal.current.proposed.questions.as_slice() {
        [_] => format!("I chose {chosen}, in {which}."),
        _ => format!("In {which}, I chose {chosen}."),
    };
    if choice.note.is_empty() { said } else { format!("{said} I wrote a note with it, which {reads}.") }
}
