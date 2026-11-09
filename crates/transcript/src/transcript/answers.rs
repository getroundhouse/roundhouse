//! Answering the agent from the transcript: its questions, by choosing on their cards or typing in the composer, and
//! its approvals.

use gpui_kit::Context;
use protocol::event::{ApprovalDecision, EventBody};
use protocol::model::{ApprovalId, Seq};

use super::{Transcript, TranscriptEvent};

impl Transcript {
    /// Records a choice on a question card, and answers once nothing else is needed.
    pub(crate) fn pick_answer(
        &mut self,
        seq: Seq,
        approval_id: ApprovalId,
        question: &crate::questions::Question,
        label: String,
        only_question: bool,
        cx: &mut Context<Self>,
    ) {
        if only_question && !question.multiple {
            let answers = std::iter::once((question.text.clone(), label)).collect();
            self.answer_approval(approval_id, ApprovalDecision::Answer { answers }, cx);
            return;
        }
        {
            let mut m = self.model.borrow_mut();
            let picks = m.picks.entry(approval_id).or_default().entry(question.text.clone()).or_default();
            crate::questions::toggle_pick(picks, label, question.multiple);
        }
        self.remeasure(seq, cx);
    }

    /// Sends what's been chosen on a card with more than one thing to choose.
    pub(crate) fn send_answers(&mut self, approval_id: ApprovalId, cx: &mut Context<Self>) {
        let picks = self.model.borrow_mut().picks.remove(&approval_id).unwrap_or_default();
        self.answer_approval(
            approval_id,
            ApprovalDecision::Answer { answers: crate::questions::answers_from(&picks) },
            cx,
        );
    }

    /// Takes text typed in the composer as the answer to an open question, if there is one: the first
    /// question on it without an answer yet. Returns false if there's nothing to answer.
    pub fn answer_with_text(&mut self, text: &str, cx: &mut Context<Self>) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        let open = self.open_approvals().into_iter().find_map(|event| {
            let EventBody::ApprovalRequested { approval_id, tool_name, input, .. } = event.body else {
                return None;
            };
            (tool_name == crate::questions::TOOL_NAME)
                .then(|| (event.seq, approval_id, crate::questions::parse(&input)))
        });
        let Some((seq, approval_id, asked)) = open else { return false };
        let done = {
            let mut m = self.model.borrow_mut();
            let picks = m.picks.entry(approval_id.clone()).or_default();
            let Some(unanswered) = crate::questions::first_unanswered(&asked, picks) else { return false };
            picks.insert(unanswered.text.clone(), vec![text.to_string()]);
            crate::questions::first_unanswered(&asked, picks).is_none()
        };
        if done {
            self.send_answers(approval_id, cx);
        } else {
            self.remeasure(seq, cx);
        }
        true
    }

    pub fn answer_approval(&mut self, approval_id: ApprovalId, decision: ApprovalDecision, cx: &mut Context<Self>) {
        let id = self.session_id.clone();
        let answering = self.client.start(move |client| client.session_approve(&id, &approval_id, decision));
        cx.spawn(async move |this, cx| {
            if let Err(e) = answering.recv().await {
                let _ = this.update(cx, |_, cx| cx.emit(TranscriptEvent::Error(format!("couldn't answer that: {e}"))));
            }
        })
        .detach();
    }
}
