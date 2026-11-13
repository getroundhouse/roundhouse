//! What the agent thought, on the same line as its calls, so the two read as one list. Closed, the line shows the
//! start of it, faint. Opened, all of it at body size, muted, under the line and on the column's left edge, and the
//! line says only that it's thinking, so the start isn't shown twice.

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement, Styled, TestSupportExt as _, div,
};

use super::line::{self, Line, Part};
use super::tools::Toggle;
use super::written;
use crate::find::Part as Text;
use crate::item_key::ItemKey;

/// What a thought's line starts with, recorded or still streaming.
pub(super) const ICON: IconName = IconName::Brain;

/// The thought, opened, as [`written`] draws it in the row it's in.
pub(super) fn draw(thought: &str, toggle: &Toggle, cx: &App) -> AnyElement {
    let summary = (!toggle.open).then(|| Part::start_of(thought));
    let line = Line::new(ICON, "Thinking").part(summary);
    let opened = || Some(text(toggle.key, written(toggle, Text::Thinking, thought, None, None), cx).into_any_element());
    line::drawn(line, None, opened, toggle, cx)
}

/// All of what the agent thought, `text`, drawn from the kit's blocks, while it's written and once it's recorded, in
/// a row of its own or a call's. Wherever it is, it's known by its [`find::view_id`].
pub(super) fn text(key: &ItemKey, text: impl IntoElement, cx: &App) -> impl IntoElement {
    div().id(key.element("thinking")).mb_1().text_color(cx.theme().muted_foreground).child(text).test_support()
}
