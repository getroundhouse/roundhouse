//! Answering the agent: its questions, asked in the composer's place by a `QuestionPanel`, and its approvals, on their
//! cards in the transcript.

use client_core::Approval;
use client_core::session_log::RecordedApprovals;
use gpui_kit::{AppContext as _, Context, Entity, Focusable as _};
use protocol::event::{ApprovalDecision, EventBody};
use protocol::model::{ApprovalId, Seq, Session};

use super::{Transcript, TranscriptEvent};
use crate::questions::QuestionPanel;

impl Transcript {
    /// The question to ask in the composer's place, while one waits on an answer.
    pub fn question(&self) -> Option<&Entity<QuestionPanel>> {
        self.asking.as_ref()
    }

    /// Follows what the session's record says of the approvals its agent is waiting on. A card whose approval opens or
    /// closes with it changes height, and so may the question asked in the composer's place.
    pub(super) fn follow_approvals(&mut self, session: &Session, cx: &mut Context<Self>) {
        let recorded = RecordedApprovals::of(session);
        if self.model.borrow().approvals == recorded {
            return;
        }
        let open = |transcript: &Self| -> Vec<Seq> {
            let m = transcript.model.borrow();
            transcript.session.read(|log| log.open_approvals(m.approvals.as_ref()).iter().map(|e| e.seq).collect())
        };
        let before = open(self);
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

    /// Asks the oldest question that's waiting, in place of whatever was asked before. The transcript draws a question
    /// only once it's no longer waiting, so each row a question leaves changes height. Several waiting at once are
    /// asked in turn.
    pub(super) fn follow_question(&mut self, cx: &mut Context<Self>) {
        let recorded = self.model.borrow().approvals.clone();
        let waiting: Vec<(Seq, ApprovalId, Approval<'static>)> = self.session.read(|log| {
            log.open_approvals(recorded.as_ref())
                .into_iter()
                .filter_map(|event| {
                    let EventBody::ApprovalRequested { approval_id, .. } = &event.body else { return None };
                    let approval = Approval::of(&event.body).filter(|approval| approval.questions().is_some())?;
                    Some((event.seq, approval_id.clone(), approval.into_owned()))
                })
                .collect()
        });
        let answered: Vec<Seq> =
            self.waiting.iter().copied().filter(|seq| !waiting.iter().any(|(still, ..)| still == seq)).collect();
        self.waiting = waiting.iter().map(|(seq, ..)| *seq).collect();
        for seq in answered {
            self.remeasure(seq, cx);
        }
        let oldest = waiting.into_iter().next();
        let held = self.asking.as_ref().map(|panel| panel.read(cx).approval_id().clone());
        if oldest.as_ref().map(|(_, id, _)| id) == held.as_ref() {
            return;
        }
        let left = self.asking.take().map(|panel| panel.focus_handle(cx));
        let this = cx.weak_entity();
        self.asking = oldest.map(|(_, id, asked)| cx.new(|cx| QuestionPanel::new(this, id, asked, cx)));
        cx.emit(TranscriptEvent::Asking { left });
        cx.notify();
    }

    pub fn answer_approval(&mut self, approval_id: ApprovalId, decision: ApprovalDecision, cx: &mut Context<Self>) {
        let id = self.session_id.clone();
        let answering_id = approval_id.clone();
        let answering = self.client.start(move |client| client.session_approve(&id, &approval_id, decision));
        cx.spawn(async move |this, cx| {
            if let Err(e) = answering.recv().await {
                let _ = this.update(cx, |t, cx| {
                    // The question is still waiting, so it can be answered again.
                    if let Some(panel) = t.asking.as_ref().filter(|p| *p.read(cx).approval_id() == answering_id) {
                        panel.update(cx, QuestionPanel::not_answered);
                    }
                    cx.emit(TranscriptEvent::Error(format!("couldn't answer that: {e}")));
                });
            }
        })
        .detach();
    }
}
