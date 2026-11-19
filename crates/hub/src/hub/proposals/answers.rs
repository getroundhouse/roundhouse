//! The person's answer to a proposal, which the hub keeps on it and tells the agent of in a prompt of its own. The
//! prompt says only what was chosen, and the agent reads the rest, such as the note, with `get_proposal`. The prompt
//! goes in the chat's queue in the transaction that keeps the answer, so the agent is told of every answer kept, once
//! it's ready for it, even after the hub restarts, and the queue sends it as it sends any.

use std::sync::Arc;

use super::super::{Hub, new_id};
use crate::store::{AnswerToKeep, Answering, now_ms};
use protocol::client::proposals::ProposalAnswer;
use protocol::client::{ApiError, HubNotice};
use protocol::model::{AnswerProblem, Choice, ClientMessageId, Proposal, ProposalRevision, Response};
use protocol::prompt::Prompt;

impl Hub {
    /// Keeps the answer, once it's checked, with the prompt that tells the agent, which the hub writes and gives an ID
    /// of its own. The same request sent again answers with the proposal as it was answered, and tells the agent
    /// nothing more. One answered by another request, or revised since the revision answered, is refused as a
    /// conflict.
    pub(crate) async fn answer_proposal(self: &Arc<Self>, answer: ProposalAnswer) -> Result<Proposal, ApiError> {
        let ProposalAnswer { id, revision, chosen, note, client_request_id } = answer;
        let lookup = id.clone();
        let proposal = self.blocking(move |store| store.get_proposal(&lookup)).await?;
        // The store checks it, against the revision as it keeps it, with whether it's answered already or revised since.
        let choice = Choice { chosen, note: note.trim().to_string(), at_ms: now_ms() };
        let session = self.live_session(&proposal.session_id).await?;
        let (answering, _) = session
            .queue_with(move |store| {
                let message = ClientMessageId::from(new_id());
                let told = |proposal: &Proposal, choice: &Choice| Prompt::new(told(proposal, choice));
                let keep =
                    AnswerToKeep { revision, choice: &choice, request: &client_request_id, message, told: &told };
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
        }
        Ok(proposal)
    }
}

/// What the agent is told: what was chosen, in which proposal, and that it reads the note with its tool.
pub(super) fn told(proposal: &Proposal, choice: &Choice) -> String {
    let answered_revision =
        ProposalRevision { response: Some(Response::Chose(choice.clone())), ..proposal.current.clone() };
    let chosen = answered_revision.chosen_in_words().unwrap_or_default();
    let title = proposal.current.proposed.title.trim();
    let mut said = match proposal.current.proposed.questions.as_slice() {
        [_] => format!("I chose {chosen}, in your proposal \"{title}\" ({}).", proposal.id),
        _ => format!("In your proposal \"{title}\" ({}), I chose {chosen}.", proposal.id),
    };
    if !choice.note.is_empty() {
        said.push_str(" I wrote a note with it, which get_proposal reads.");
    }
    said
}
