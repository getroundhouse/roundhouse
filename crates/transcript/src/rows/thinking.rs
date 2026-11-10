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
use crate::item_key::ItemKey;

/// What a thought's line starts with, recorded or still streaming.
pub(super) const ICON: IconName = IconName::Brain;

pub(super) fn draw(thought: &str, toggle: &Toggle, cx: &App) -> AnyElement {
    let summary = (!toggle.open).then(|| Part::start_of(thought));
    let line = Line::new(ICON, "Thinking").part(summary);
    let opened = toggle.open.then(|| text(toggle.key, thought, cx).into_any_element());
    line::drawn(line, None, opened, toggle, cx)
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
