//! Answering the agent: its questions and its requests to use a tool, asked in the composer's place by a
//! `QuestionPanel`, and its plans, on their cards in the transcript.

use client_core::Approval;
use client_core::session_log::RecordedApprovals;
use gpui_kit::{AppContext as _, Context, Entity, Focusable as _};
use protocol::capability::approvals::Decision;
use protocol::event::EventBody;
use protocol::model::{ApprovalId, Seq, Session, SubagentId};

use super::{Model, Transcript, TranscriptEvent};
use crate::questions::{Asks, QuestionPanel};

/// A request waiting to be asked in the composer's place.
struct Waiting {
    seq: Seq,
    approval_id: ApprovalId,
    asks: Asks,
    /// The subagent that asks.
    asked_by: Option<SubagentId>,
}

/// What a subagent that asks is called where its record doesn't say what it's for, or hasn't come.
const A_SUBAGENT: &str = "Subagent";

impl Transcript {
    /// What to ask in the composer's place, while a question or a tool's use waits on an answer.
    pub fn question(&self) -> Option<&Entity<QuestionPanel>> {
        self.asking.as_ref()
    }

    /// Follows what the session's record says of the approvals its agent is waiting on. A plan's card whose approval
    /// opens or closes with it changes height, and so may what's asked in the composer's place.
    pub(super) fn follow_approvals(&mut self, session: &Session, cx: &mut Context<Self>) {
        let recorded = RecordedApprovals::of(session);
        if self.model.borrow().approvals == recorded {
            return;
        }
        let open = |transcript: &Self| -> Vec<Seq> {
            let m = transcript.model.borrow();
            transcript.log.open_approvals(&m.approvals).iter().map(|e| e.seq).collect()
        };
        let before = open(self);
        self.session.waiting_on(&recorded);
        self.model.borrow_mut().approvals = recorded;
        let after = open(self);
        if before == after {
            return;
        }
        let changed: Vec<Seq> =
            before.iter().chain(&after).copied().filter(|seq| before.contains(seq) != after.contains(seq)).collect();
        for seq in changed {
            self.remeasure(seq, cx);
        }
        self.follow_question(cx);
    }

    /// Asks the oldest question or tool's use that's waiting, in place of whatever was asked before, and says how many
    /// wait behind it. The transcript draws a question only once it's no longer waiting, so each row a question leaves
    /// changes height. Several waiting at once are asked in turn. A plan isn't asked here: its card is in the
    /// transcript, with the plan.
    pub(super) fn follow_question(&mut self, cx: &mut Context<Self>) {
        let waiting = self.waiting_to_be_asked();
        let answered: Vec<Seq> =
            self.waiting.iter().copied().filter(|seq| !waiting.iter().any(|still| still.seq == *seq)).collect();
        self.waiting = waiting.iter().map(|waiting| waiting.seq).collect();
        for seq in answered {
            self.remeasure(seq, cx);
        }
        let oldest = waiting.into_iter().next();
        let held = self.asking.as_ref().map(|panel| (panel.read(cx).approval_id().clone(), panel.read(cx).can_type()));
        if oldest.as_ref().map(|oldest| (oldest.approval_id.clone(), oldest.asks.can_type(0))) != held {
            let left = self.asking.take().map(|panel| panel.focus_handle(cx));
            let this = cx.weak_entity();
            self.asking = oldest.map(|Waiting { approval_id, asks, asked_by, .. }| {
                cx.new(|cx| QuestionPanel::new(this, approval_id, asks, asked_by, cx))
            });
            cx.emit(TranscriptEvent::Asking { left });
            cx.notify();
        }
        self.tell_the_panel(cx);
    }

    /// Tells the panel what's around its request: what the subagent that asks is for, by its record, and how many
    /// more requests wait behind it. Both can change while the request waits, as a record comes or another agent asks.
    pub(super) fn tell_the_panel(&self, cx: &mut Context<Self>) {
        let Some(panel) = &self.asking else { return };
        let asker = panel.read(cx).asked_by().map(|subagent| self.model.borrow().subagent_called(subagent));
        let more_waiting = self.waiting.len().saturating_sub(1);
        panel.update(cx, |panel, cx| panel.set_around(asker, more_waiting, cx));
    }

    /// The requests waiting that are asked in the composer's place, oldest first: every one but a plan.
    fn waiting_to_be_asked(&self) -> Vec<Waiting> {
        let m = self.model.borrow();
        self.log
            .open_approvals(&m.approvals)
            .into_iter()
            .filter_map(|event| {
                let EventBody::ApprovalRequested { approval_id, .. } = &event.body else { return None };
                let (seq, approval_id) = (event.seq, approval_id.clone());
                let asked_by = Asks::asked_by(&event.body).cloned();
                Some(Waiting { seq, approval_id, asks: Asks::of(&event.body, m.claims_of(event.run))?, asked_by })
            })
            .collect()
    }

    /// Approves the plan that waits under `approval_id`, or keeps planning, as `approve` says.
    pub(crate) fn answer_plan(&mut self, approval_id: ApprovalId, approve: bool, cx: &mut Context<Self>) {
        let decision = {
            let m = self.model.borrow();
            let open = self.log.open_approvals(&m.approvals);
            let asked = open.iter().find(
                |event| matches!(&event.body, EventBody::ApprovalRequested { approval_id: id, .. } if *id == approval_id),
            );
            let Some(approval) = asked.and_then(|event| Approval::of(&event.body)) else { return };
            if approve { approval.approve() } else { Some(approval.refuse(None)) }
        };
        match decision {
            Some(decision) => self.answer_approval(approval_id, decision, cx),
            None => {
                cx.emit(TranscriptEvent::Error("Couldn't approve that: it asks for more than a yes".into()));
            }
        }
    }

    pub fn answer_approval(&mut self, approval_id: ApprovalId, decision: Decision, cx: &mut Context<Self>) {
        let id = self.session_id.clone();
        let answering_id = approval_id.clone();
        let answering = self.client.start(move |client| client.session_decide(&id, &approval_id, decision));
        cx.spawn(async move |this, cx| {
            if let Err(e) = answering.recv().await {
                let _ = this.update(cx, |t, cx| {
                    // The question is still waiting, so it can be answered again.
                    if let Some(panel) = t.asking.as_ref().filter(|p| *p.read(cx).approval_id() == answering_id) {
                        panel.update(cx, QuestionPanel::not_answered);
                    }
                    cx.emit(TranscriptEvent::Error(client_core::refusals::couldnt("answer that", &e)));
                });
            }
        })
        .detach();
    }
}

impl Model {
    /// What `subagent` is called, to say who asks: what it's for, or its kind, or `Subagent` where its record says
    /// neither or hasn't come.
    pub(crate) fn subagent_called(&self, subagent: &SubagentId) -> String {
        self.subagents.called(subagent).unwrap_or_else(|| A_SUBAGENT.to_string())
    }
}
