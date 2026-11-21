//! The note dictates as a question's written answer does, with the shared keys and live line and no microphone. The
//! app says whether the tab is in sight, and the note dictates only then, and only while the revision shown waits for
//! an answer and none is being sent, since what went in meanwhile would be lost once it's answered. It goes on while
//! a newer revision is held back, as `held` says, since the note goes on to that one. The view outlives being out of
//! sight, so going out of sight ends a recording, keeping what went in.

use client_core::hints::HintCollector;
use gpui_kit::component::input::TextareaState;
use gpui_kit::{App, Context, Entity, Subscription, Window};

use super::answering::ProposalEvent;
use super::view::ProposalView;

impl ProposalView {
    /// Whether the tab is in sight: in front, in a workspace that's showing. Its note is dictated into only then, and
    /// a revision the agent writes then doesn't take the page, as `held` says.
    pub fn set_in_sight(&mut self, in_sight: bool, cx: &mut Context<Self>) {
        self.in_sight = in_sight;
        self.follow_dictation(cx);
    }

    /// Gives the note's dictation the app's hint words, after the proposal's own.
    pub fn set_dictation_hints(&mut self, hints: impl Fn(&mut HintCollector, &App) + 'static) {
        self.dictation_hints = Some(Box::new(hints));
    }

    fn dictates(&self) -> bool {
        self.in_sight && self.takes_a_note()
    }

    /// Lets the note dictate while the tab's in sight and the note can be written in, and otherwise ends its
    /// recording.
    pub(super) fn follow_dictation(&mut self, cx: &mut Context<Self>) {
        let dictates = self.dictates();
        if let Some(note) = &self.answering.note {
            note.dictation.update(cx, |field, cx| field.set_enabled(dictates, cx));
        }
    }

    /// Ends the note's recording, keeping what went in, without sending.
    pub(super) fn end_dictation(&mut self, cx: &mut Context<Self>) {
        if let Some(note) = &self.answering.note {
            note.dictation.update(cx, dictation_ui::Field::end);
        }
    }

    /// Whether Send waits for the last words dictated into the note. The recording ends asking to send only when this
    /// asked it to, and Send is the note's one action, so that's when it sends.
    pub(super) fn once_dictated(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(field) = self.answering.note.as_ref().map(|note| note.dictation.clone()) else { return false };
        let ready = self.ready().is_some();
        field.update(cx, |field, cx| field.finish_before_submit(ready, cx))
    }

    fn dictation_event(&mut self, event: &dictation_ui::Event, cx: &mut Context<Self>) {
        match event {
            dictation_ui::Event::Error(message) => cx.emit(ProposalEvent::Error(message.clone())),
            dictation_ui::Event::Ended { submit: true } => self.send(cx),
            dictation_ui::Event::Ended { submit: false } | dictation_ui::Event::Started => {}
        }
        cx.notify();
    }

    /// Dictation for the note `input`.
    pub(super) fn attach_dictation(
        &self,
        input: &Entity<TextareaState>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> (Entity<dictation_ui::Field>, [Subscription; 2]) {
        let on_event = |view: &mut Self, event: &dictation_ui::Event, _: &mut Window, cx: &mut Context<Self>| {
            view.dictation_event(event, cx);
        };
        dictation_ui::Field::attach(input, look::Corner::Card, self.dictates(), Self::hints, on_event, window, cx)
    }

    /// The words to favour in the note: what's typed in it, the proposal's, then what the app adds.
    fn hints(&self, cx: &App) -> dictation_ui::Hints {
        let mut collector = HintCollector::new();
        if let Some(note) = &self.answering.note {
            collector.add_prose(&note.input.read(cx).text().to_string());
        }
        collector.add_proposal(&self.proposal.current.proposed);
        if let Some(hints) = &self.dictation_hints {
            hints(&mut collector, cx);
        }
        dictation_ui::Hints::new(collector.finish())
    }

    /// Whether the note is being dictated into.
    #[cfg(feature = "testing")]
    pub fn dictating(&self, cx: &App) -> bool {
        self.answering.note.as_ref().is_some_and(|note| note.dictation.read(cx).is_dictating())
    }

    /// What's written in the note.
    #[cfg(feature = "testing")]
    pub fn note_text(&self, cx: &App) -> String {
        self.answering.note.as_ref().map_or_default(|note| note.input.read(cx).text().to_string())
    }
}
