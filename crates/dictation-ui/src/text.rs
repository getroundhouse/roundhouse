//! Final words and cancellation use the input's editing operations, which emit its usual change event and preserve
//! undo history. Setting its value instead would silently bypass draft saving and answer selection.

use gpui_kit::component::input::{Position, Rope, TextareaState};
use gpui_kit::{Context, Window};

use crate::Field;

pub(crate) struct Before {
    pub(crate) text: Rope,
    cursor: Position,
}

impl Before {
    pub(crate) fn of(input: &TextareaState) -> Self {
        Self { text: input.text().clone(), cursor: input.cursor_position() }
    }
}

impl Field {
    pub(crate) fn put_back(&mut self, before: &Before, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.replace_all(before.text.to_string(), window, cx);
            input.set_cursor_position(before.cursor, window, cx);
        });
    }

    pub(crate) fn put_in(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            let typed = input.text().to_string();
            let (before, after) =
                (typed.get(..input.cursor()).unwrap_or(&typed), typed.get(input.cursor()..).unwrap_or(""));
            let apart = |next: Option<char>| if next.is_some_and(|c| !c.is_whitespace()) { " " } else { "" };
            let text = format!("{}{text}{}", apart(before.chars().last()), apart(after.chars().next()));
            input.insert(text, window, cx);
        });
    }
}
