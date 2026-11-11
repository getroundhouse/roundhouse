//! What the agent thought, on the same line as its calls, so the two read as one list. Closed, the line shows the
//! start of it, faint. Opened, all of it at body size, muted, under the line and on the column's left edge, and the
//! line says only that it's thinking, so the start isn't shown twice.

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, Styled, TestSupportExt as _, div,
};

use super::line::{self, Line, Part};
use super::tools::Toggle;
use crate::find::{self, Part as Text};
use crate::item_key::ItemKey;

/// What a thought's line starts with, recorded or still streaming.
pub(super) const ICON: IconName = IconName::Brain;

pub(super) fn draw(thought: &str, toggle: &Toggle, cx: &App) -> AnyElement {
    let summary = (!toggle.open).then(|| Part::start_of(thought));
    let line = Line::new(ICON, "Thinking").part(summary);
    let opened = toggle.open.then(|| {
        let (id, view) =
            ((toggle.key.clone(), Text::Thinking), toggle.shared.texts.markdown(text_id(toggle.key), thought));
        text(toggle.key, find::markdown_view(view, &id, toggle.find, None), cx).into_any_element()
    });
    line::drawn(line, None, opened, toggle, cx)
}

/// All of what the agent thought, in its text view, which is under [`text_id`] while it streams and once it's
/// recorded.
pub(super) fn text(key: &ItemKey, text_view: impl IntoElement, cx: &App) -> impl IntoElement {
    div().id(key.element("thinking")).mb_1().text_color(cx.theme().muted_foreground).child(text_view).test_support()
}

/// What all of the thought under `key` is drawn by.
pub(super) fn text_id(key: &ItemKey) -> ElementId {
    key.element("thinking-text")
}
