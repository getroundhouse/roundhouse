//! Following the chat's proposals, which a `propose` call's line is drawn from, found by the call the hub recorded as
//! making each, and the line of the prompt that told the agent of each one's answer, which names it.

use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::Context;
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Proposal, ProposalId, ToolUseId};

use super::Transcript;
use super::model::Wrote;

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
        let by_call: HashMap<ToolUseId, Wrote> = proposals.iter().flat_map(written_by).collect();
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

/// Each revision of `proposal` a call wrote, by the call. A hub from before revisions sums none up, and its proposals
/// have only their first, the newest.
fn written_by(proposal: &Arc<Proposal>) -> Vec<(ToolUseId, Wrote)> {
    let wrote = |revision| Wrote { proposal: Arc::clone(proposal), revision };
    if proposal.revisions.is_empty() {
        let newest = proposal.current.call.clone().map(|call| (call, wrote(proposal.current.number)));
        return newest.into_iter().collect();
    }
    proposal.revisions.iter().filter_map(|revision| Some((revision.call.clone()?, wrote(revision.number)))).collect()
}

/// Whether `block` is a call that wrote one of the proposals `changed`.
fn makes(block: &ContentBlock, by_call: &HashMap<ToolUseId, Wrote>, changed: &[ProposalId]) -> bool {
    match block {
        ContentBlock::Call { id, .. } => by_call.get(id).is_some_and(|wrote| changed.contains(&wrote.proposal.id)),
        ContentBlock::Text { .. }
        | ContentBlock::Thinking { .. }
        | ContentBlock::CallResult { .. }
        | ContentBlock::Image { .. }
        | ContentBlock::Unknown => false,
    }
}
