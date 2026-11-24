//! Following the chat's proposals, which a `propose` call's line is drawn from, found by the call the hub recorded as
//! making each, and so is each call that revised or withdrew one, and the line of the prompt that told the agent of
//! each one's answer, which names it.

use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::Context;
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Proposal, ProposalId, RevisionSummary, ToolUseId};

use super::Transcript;
use super::model::{Did, ProposalCall};

impl Transcript {
    /// Follows the records of the chat's proposals, as client-core's snapshot shares them, and measures again each
    /// row that draws one that changed: the call that made it, and the prompt that told the agent of its answer. A
    /// copy that's shared already is the one held.
    pub fn set_proposals(&mut self, proposals: &[Arc<Proposal>], cx: &mut Context<Self>) {
        let changed: Vec<ProposalId> = {
            let m = self.model.borrow();
            let held = |proposal: &Arc<Proposal>| {
                m.proposals_by_id.get(&proposal.id).is_some_and(|held| Arc::ptr_eq(held, proposal))
            };
            proposals.iter().filter(|proposal| !held(proposal)).map(|proposal| proposal.id.clone()).collect()
        };
        if changed.is_empty() {
            return;
        }
        let by_call: HashMap<ToolUseId, ProposalCall> = proposals.iter().flat_map(calls_on).collect();
        let by_id: HashMap<ProposalId, Arc<Proposal>> =
            proposals.iter().map(|proposal| (proposal.id.clone(), Arc::clone(proposal))).collect();
        let draws = |event: &SessionEvent| {
            let made = crate::rows::blocks_of(&event.body).iter().any(|block| makes(block, &by_call, &changed));
            let EventBody::UserPrompt { from, .. } = &event.body else { return made };
            from.responds_to().is_some_and(|answers| changed.contains(answers))
        };
        // Found among what's loaded, since a proposal is made rarely, and a row is measured only once it's loaded.
        let rows: Vec<_> = self.log.loaded().filter(|event| draws(event)).map(|event| event.seq).collect();
        {
            let mut m = self.model.borrow_mut();
            m.proposals = by_call;
            m.proposals_by_id = by_id;
        }
        for seq in rows {
            self.remeasure(seq, cx);
        }
        cx.notify();
    }
}

/// What each call did to `proposal`, by the call: each revision it wrote, and its withdrawal.
fn calls_on(proposal: &Arc<Proposal>) -> Vec<(ToolUseId, ProposalCall)> {
    let did = |did| ProposalCall { proposal: Arc::clone(proposal), did };
    let wrote = |revision: &RevisionSummary| Some((revision.call.clone()?, did(Did::Wrote(revision.number))));
    let wrote: Vec<_> = proposal.revisions.iter().filter_map(wrote).collect();
    let withdrew =
        proposal.withdrawn.as_ref().and_then(|withdrawal| Some((withdrawal.call.clone()?, did(Did::Withdrew))));
    wrote.into_iter().chain(withdrew).collect()
}

/// Whether `block` is a call that wrote one of the proposals `changed`.
fn makes(block: &ContentBlock, by_call: &HashMap<ToolUseId, ProposalCall>, changed: &[ProposalId]) -> bool {
    match block {
        ContentBlock::Call { id, .. } => by_call.get(id).is_some_and(|call| changed.contains(&call.proposal.id)),
        ContentBlock::Text { .. }
        | ContentBlock::Thinking { .. }
        | ContentBlock::CallResult { .. }
        | ContentBlock::Image { .. }
        | ContentBlock::Unknown => false,
    }
}
