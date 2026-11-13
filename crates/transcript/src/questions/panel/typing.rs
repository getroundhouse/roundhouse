//! The row under the options for typing another answer, which is chosen while there's any text in it.

use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};

use super::QuestionPanel;

pub(super) struct Other {
    pub(super) input: Entity<TextareaState>,
    _events: Subscription,
}

impl QuestionPanel {
    pub(super) fn type_other(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.other(self.showing, window, cx);
        input.update(cx, |input, cx| input.focus(window, cx));
    }

    /// The question's row for typing another answer. What's typed there is chosen while there's any, and Enter goes on.
    pub(super) fn other(&mut self, q: usize, window: &mut Window, cx: &mut Context<Self>) -> Entity<TextareaState> {
        if let Some(other) = self.asked.get(q).and_then(|asked| asked.other.as_ref()) {
            return other.input.clone();
        }
        let says = self.asks.typing_row_says();
        let input = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 6).submit_on_enter(true).placeholder(says));
        let events = cx.subscribe_in(&input, window, move |this, input, event: &InputEvent, window, cx| match event {
            // Going back to what's typed chooses it again.
            InputEvent::Change | InputEvent::Focus => {
                let has_text = !input.read(cx).text().to_string().trim().is_empty();
                let chosen = this.asked.get_mut(q).is_some_and(|asked| {
                    let before = asked.choice.clone();
                    asked.choice.typed(has_text, asked.question.multiple);
                    asked.choice != before
                });
                // Only what changes is kept, so going back to the row doesn't hold back a change made elsewhere.
                if chosen || matches!(event, InputEvent::Change) {
                    this.answering(false, cx);
                }
                cx.notify();
            }
            InputEvent::PressEnter { shift: false, .. } => this.enter_in_the_typing_row(q, window, cx),
            InputEvent::PressEnter { .. } | InputEvent::Blur => {}
        });
        if let Some(asked) = self.asked.get_mut(q) {
            asked.other = Some(Other { input: input.clone(), _events: events });
        }
        input
    }

    /// Enter in the typing row goes on, as it does anywhere in the panel. With nothing typed there, where [`Asks`]
    /// says an option chosen before mustn't be sent from the row, it leaves the row instead, as Escape does, and the
    /// panel ignores keys for a moment.
    ///
    /// [`Asks`]: super::Asks
    fn enter_in_the_typing_row(&mut self, q: usize, window: &mut Window, cx: &mut Context<Self>) {
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
