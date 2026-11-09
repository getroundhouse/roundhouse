//! What the agent thought, on the same line as its calls, so the two read as one list. Closed, the line shows the
//! start of it, faint. Opened, all of it at body size, muted, under the line and on the column's left edge, and the
//! line says only that it's thinking, so the start isn't shown twice.

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::text::TextView;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement, SharedString, Styled, TestSupportExt as _,
    div,
};

use super::line::{self, Line, Part};
use super::tools::Toggle;
use crate::calls::first_line;
use crate::item_key::ItemKey;

/// What a thought's line starts with, recorded or still streaming.
pub(super) const ICON: IconName = IconName::Brain;

/// More of a thought's start than its line has room for, so a long first line isn't copied to be cut off.
const START_CHARS: usize = 160;

pub(super) fn draw(thought: &str, toggle: &Toggle, cx: &App) -> AnyElement {
    let summary = (!toggle.open).then(|| Part::Summary(start(thought).into()));
    let line = Line::new(ICON, "Thinking").part(summary);
    let opened = toggle.open.then(|| text(toggle.key, thought, cx).into_any_element());
    line::drawn(line, None, opened, toggle, cx)
}

/// The start of a thought, for its line: its first line with anything on it, without the marks for code and
/// emphasis, which the line can't draw.
fn start(thought: &str) -> String {
    first_line(thought).chars().filter(|c| !matches!(c, '`' | '*')).take(START_CHARS).collect()
}

/// All of what the agent thought, under the same ID while it streams and once it's recorded.
pub(super) fn text(key: &ItemKey, thought: &str, cx: &App) -> impl IntoElement {
    let text = TextView::markdown(key.element("thinking-text"), SharedString::from(thought.to_string()));
    div()
        .id(key.element("thinking"))
        .mb_1()
        .text_color(cx.theme().muted_foreground)
        .child(text.selectable(true))
        .test_support()
}

#[cfg(test)]
mod tests {
    use super::start;

    #[test]
    fn the_start_of_a_thought_leaves_out_markdown_s_marks() {
        assert_eq!(start("`slow` takes a **target** speed\nand more"), "slow takes a target speed");
    }

    #[test]
    fn a_thought_that_starts_with_a_blank_line_starts_where_its_words_do() {
        assert_eq!(start("\n  \nThe test says a train should slow."), "The test says a train should slow.");
    }
}
