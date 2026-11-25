//! Question answers opt into shared shortcuts and live text, with no microphone button. Each input keeps its own
//! attachment; navigation and hiding explicitly end it because the panel retains its answers while out of sight.

use client_core::hints::{Around, Field};
use gpui_kit::component::input::TextareaState;
use gpui_kit::{App, Context, Entity, Window};
use protocol::capability::asking::QuestionAnswer;

use super::QuestionPanel;
use crate::{Transcript, TranscriptEvent};

#[derive(Clone, Copy)]
pub(super) enum Then {
    Send,
    GoOn,
    Typed,
}

impl QuestionPanel {
    pub(super) fn dictation(&self) -> Option<&Entity<dictation_ui::Field>> {
        self.asked.get(self.showing)?.other.as_ref()?.dictation.as_ref()
    }

    /// The host says whether this panel is drawn, even while its transcript and inputs remain alive elsewhere.
    pub fn set_dictation_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.dictation_enabled = enabled;
        for (q, asked) in self.asked.iter().enumerate() {
            if let Some(field) = asked.other.as_ref().and_then(|other| other.dictation.as_ref()) {
                field.update(cx, |field, cx| field.set_enabled(enabled && q == self.showing && !self.answered, cx));
            }
        }
    }

    pub(super) fn end_dictation(&mut self, cx: &mut Context<Self>) {
        if let Some(field) = self.dictation().cloned() {
            field.update(cx, dictation_ui::Field::end);
        }
    }

    pub(super) fn once_dictated(&mut self, then: Then, cx: &mut Context<Self>) -> bool {
        let content = self.answers_so_far(cx).iter().any(Option::is_some);
        let Some(field) = self.dictation().cloned() else { return false };
        self.after_dictation.wait(&field, content, then, cx)
    }

    /// What's dictated into question `q`'s typed answer, `typed`, is about, with the words to favour: what's typed
    /// there, the question, then the workspace's and the chat's.
    pub(super) fn answer_about(&self, q: usize, typed: &Entity<TextareaState>, cx: &App) -> dictation_ui::About {
        let Some(asked) = self.asked.get(q) else { return dictation_ui::About::default() };
        let typed = typed.read(cx).text().to_string();
        let transcript = self.transcript.upgrade();
        let around = transcript.as_ref().map_or_else(Around::default, |t| t.read(cx).dictation_around(cx));
        let within = within(transcript.as_ref(), &around, cx);
        let picked = Field::Answer { typed: &typed, question: &asked.question, around }.picked();
        dictation_ui::About::new(within, picked)
    }

    /// Says that what's typed in `answers` was sent, for what's kept of dictation. The questions are answered at once,
    /// so it's one thing sent.
    pub(super) fn typed_sent(&self, answers: &[QuestionAnswer], cx: &mut Context<Self>) {
        let Some(field) = self.asked.iter().find_map(|asked| asked.other.as_ref()?.dictation.clone()) else { return };
        let typed: Vec<&str> = answers.iter().filter_map(|answer| answer.typed.as_deref()).collect();
        let transcript = self.transcript.upgrade();
        let around = transcript.as_ref().map_or_else(Around::default, |t| t.read(cx).dictation_around(cx));
        let within = within(transcript.as_ref(), &around, cx);
        field.update(cx, |field, cx| field.sent(within, typed.join("\n\n"), cx));
    }

    pub(super) fn dictation_event(
        &mut self,
        q: usize,
        event: &dictation_ui::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if q != self.showing {
            return;
        }
        match event {
            dictation_ui::Event::Started => {}
            dictation_ui::Event::Error(message) => {
                let _ = self.transcript.update(cx, |_, cx| cx.emit(TranscriptEvent::Error(message.clone())));
            }
            dictation_ui::Event::Ended { submit } => {
                let then = self.after_dictation.ended(*submit).filter(|_| self.dictation_enabled && !self.answered);
                match then {
                    Some(Then::Send) => self.send(cx),
                    Some(Then::GoOn) => self.go_on(window, cx),
                    Some(Then::Typed) => self.enter_in_the_typing_row(q, window, cx),
                    None => {}
                }
            }
        }
        cx.notify();
    }
}

/// Where an answer in `transcript`, with `around` it, is.
fn within(transcript: Option<&Entity<Transcript>>, around: &Around<'_>, cx: &App) -> dictation_ui::Within {
    dictation_ui::Within {
        workspace: around.workspace.as_ref().map(|workspace| workspace.shown.workspace().id.to_string()),
        chat: transcript.map(|t| t.read(cx).session_id().to_string()),
        said_out_loud: false,
    }
}
