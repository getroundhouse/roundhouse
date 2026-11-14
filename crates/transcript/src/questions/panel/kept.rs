//! What's chosen and typed in the panel and not sent, which the hub keeps until what's asked is answered, so it's
//! there when the app opens again and in any other client.

use gpui_kit::{App, Context, Window};
use protocol::model::{AnswerDraft, AnswerSoFar};

use super::QuestionPanel;
use crate::questions::choice::Choice;
use crate::transcript::TranscriptEvent;

impl QuestionPanel {
    /// Something's typed in a row for another answer, which would be lost with what's asked.
    pub fn has_typed(&self, cx: &App) -> bool {
        self.asked.iter().any(|asked| !asked.typed(cx).trim().is_empty())
    }

    /// What's chosen and typed so far, as the hub keeps it, or `None` with nothing, or once it's answered. Options are
    /// kept by their places, which for a tool's use are those of `client_core::ToolUseAnswer::ALL`, so each client
    /// reads them alike.
    pub fn so_far(&self, cx: &App) -> Option<AnswerDraft> {
        if self.answered {
            return None;
        }
        let answers: Vec<AnswerSoFar> = self
            .asked
            .iter()
            .enumerate()
            .map(|(index, asked)| {
                if self.asks.secret(index) {
                    return AnswerSoFar::default();
                }
                let (picked, other) = asked.choice.kept();
                let picked = picked.iter().map(|&option| u32::try_from(option).unwrap_or(u32::MAX)).collect();
                AnswerSoFar { picked, typed: asked.typed(cx), other }
            })
            .collect();
        let draft = AnswerDraft { approval_id: self.approval_id.clone(), answers };
        draft.says_anything().then_some(draft)
    }

    /// Takes what was chosen and typed as the hub kept it, from before the app last stopped or from another client, in
    /// place of what's here, or clears it for `None`. The hub has it already, so it isn't kept again, and the focus
    /// stays where it is. A choice that changes ignores keys for the half second the panel does when it's just come,
    /// so an Enter meant for what was chosen here can't send what was chosen elsewhere.
    pub fn take_so_far(&mut self, kept: Option<&AnswerDraft>, window: &mut Window, cx: &mut Context<Self>) {
        for q in 0..self.asked.len() {
            if self.asks.secret(q) {
                continue;
            }
            let so_far = kept.and_then(|kept| kept.answers.get(q)).cloned().unwrap_or_default();
            let Some(asked) = self.asked.get_mut(q) else { continue };
            let picked = so_far.picked.iter().filter_map(|&option| usize::try_from(option).ok());
            let choice = Choice::from_kept(picked, so_far.other, asked.question.options.len());
            if choice != asked.choice {
                self.quiet_until = cx.background_executor().now() + super::SETTLE;
            }
            asked.choice = choice;
            if so_far.typed.is_empty() && asked.other.is_none() {
                continue;
            }
            let input = self.other(q, window, cx);
            input.restore(so_far.typed, window, cx);
        }
        cx.notify();
    }

    /// Asks for what's chosen and typed to be kept, as [`TranscriptEvent::Answering`] says.
    pub(super) fn answering(&self, at_once: bool, cx: &mut Context<Self>) {
        let _ = self.transcript.update(cx, |_, cx| cx.emit(TranscriptEvent::Answering { at_once }));
    }
}
