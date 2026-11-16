//! The row under the options for typing another answer, which is chosen while there's any text in it.

use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::{App, AppContext as _, Context, Entity, Subscription, Window};

use super::QuestionPanel;

pub(super) struct Other {
    pub(super) input: Field,
    pub(super) dictation: Option<Entity<dictation_ui::Field>>,
    _events: Vec<Subscription>,
}

impl QuestionPanel {
    pub(super) fn type_other(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.asks.can_type(self.showing) {
            return;
        }
        self.other(self.showing, window, cx).focus(window, cx);
    }

    /// The question's row for typing another answer. What's typed there is chosen while there's any, and Enter goes on.
    pub(super) fn other(&mut self, q: usize, window: &mut Window, cx: &mut Context<Self>) -> Field {
        if let Some(other) = self.asked.get(q).and_then(|asked| asked.other.as_ref()) {
            return other.input.clone();
        }
        let says = self.asks.typing_row_says();
        let (input, events) = if self.asks.secret(q) {
            let input = cx.new(|cx| InputState::new(window, cx).masked(true).placeholder("Secret answer"));
            let events = cx.subscribe_in(&input, window, move |this, input, event: &InputEvent, window, cx| {
                let has_text = !input.read(cx).text().to_string().trim().is_empty();
                this.typed_changed(q, has_text, event, window, cx);
            });
            (Field::Secret(input), events)
        } else {
            let input =
                cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 6).submit_on_enter(true).placeholder(says));
            let events = cx.subscribe_in(&input, window, move |this, input, event: &InputEvent, window, cx| {
                let has_text = !input.read(cx).text().to_string().trim().is_empty();
                this.typed_changed(q, has_text, event, window, cx);
            });
            (Field::Text(input), events)
        };
        let mut events = vec![events];
        let dictation = if let Field::Text(input) = &input
            && self.asks.can_type(q)
        {
            let panel = cx.weak_entity();
            let typed_input = input.clone();
            let field = cx.new(|cx| {
                let mut field =
                    dictation_ui::Field::new(input.clone(), look::Corner::Control, window, cx).with_hints(move |cx| {
                        let mut collector = client_core::hints::HintCollector::new();
                        collector.add_prose(&typed_input.read(cx).text().to_string());
                        if let Some(panel) = panel.upgrade() {
                            let panel = panel.read(cx);
                            if let Some(asked) = panel.asked.get(q) {
                                collector.add_question(&asked.question);
                            }
                            if let Some(transcript) = panel.transcript.upgrade() {
                                let transcript = transcript.read(cx);
                                transcript.add_dictation_hints(&mut collector, cx);
                                collector.add_session_log(transcript.log());
                            }
                        }
                        dictation_ui::Hints::new(collector.finish())
                    });
                field.set_enabled(self.dictation_enabled && q == self.showing, cx);
                field
            });
            events.push(cx.subscribe_in(&field, window, move |this, _, event, window, cx| {
                this.dictation_event(q, event, window, cx);
            }));
            events.push(cx.observe(&field, |_, _, cx| cx.notify()));
            Some(field)
        } else {
            None
        };
        if let Some(asked) = self.asked.get_mut(q) {
            asked.other = Some(Other { input: input.clone(), dictation, _events: events });
        }
        input
    }

    fn typed_changed(
        &mut self,
        q: usize,
        has_text: bool,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change | InputEvent::Focus => {
                let chosen = self.asked.get_mut(q).is_some_and(|asked| {
                    let before = asked.choice.clone();
                    asked.choice.typed(has_text, asked.question.multiple);
                    asked.choice != before
                });
                if chosen || matches!(event, InputEvent::Change) {
                    self.answering(false, cx);
                }
                cx.notify();
            }
            InputEvent::PressEnter { shift: false, .. } => self.enter_in_the_typing_row(q, window, cx),
            InputEvent::PressEnter { .. } | InputEvent::Blur => {}
        }
    }

    /// Enter in the typing row goes on, as it does anywhere in the panel. With nothing typed there, where [`Asks`]
    /// says an option chosen before mustn't be sent from the row, it leaves the row instead, as Escape does, and the
    /// panel ignores keys for a moment.
    ///
    /// [`Asks`]: super::Asks
    pub(super) fn enter_in_the_typing_row(&mut self, q: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.once_dictated(super::dictating::Then::Typed, cx) {
            return;
        }
        let typed = self.asked.get(q).is_some_and(|asked| asked.choice.other());
        if typed || self.asks.sends_a_chosen_option_from_the_typing_row() {
            self.go_on(window, cx);
        } else {
            self.show(q, window, cx);
            // Quiet again, as when it's just come, so a second Enter, or one held down, sends nothing either.
            self.quiet_until = cx.background_executor().now() + super::SETTLE;
        }
    }
}

/// The usual multiline answer, or an input whose secret is masked by the input engine.
#[derive(Clone)]
pub(super) enum Field {
    Text(Entity<TextareaState>),
    Secret(Entity<InputState>),
}

impl Field {
    pub(super) fn text(&self, cx: &App) -> String {
        match self {
            Self::Text(input) => input.read(cx).text().to_string(),
            Self::Secret(input) => input.read(cx).text().to_string(),
        }
    }
    fn focus(&self, window: &mut Window, cx: &mut App) {
        match self {
            Self::Text(input) => input.update(cx, |i, cx| i.focus(window, cx)),
            Self::Secret(input) => input.update(cx, |i, cx| i.focus(window, cx)),
        }
    }
    pub(super) fn restore(&self, value: String, window: &mut Window, cx: &mut App) {
        // Secret inputs are never restored from a shared draft.
        if let Self::Text(input) = self {
            input.update(cx, |i, cx| {
                let end = value.len();
                i.set_value(value, window, cx);
                i.set_selected_range(end..end, cx);
            });
        }
    }
}
