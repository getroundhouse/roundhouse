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
use super::{InRow, recorded};
use crate::find::{self, Part as Text};
use crate::item_key::ItemKey;
use crate::prose::TextAt;

/// What a thought's line starts with, recorded or still streaming.
pub(super) const ICON: IconName = IconName::Brain;

/// The thought, in the row `in_row` says. In a row of its own, opened, the transcript draws it from the kit's
/// blocks, and selects it itself. In a call's row, as a subagent's is, it's the kit's text view.
pub(super) fn draw(thought: &str, toggle: &Toggle, in_row: InRow, cx: &App) -> AnyElement {
    let summary = (!toggle.open).then(|| Part::start_of(thought));
    let line = Line::new(ICON, "Thinking").part(summary);
    let opened = toggle.open.then(|| {
        let (shared, id) = (toggle.shared, (toggle.key.clone(), Text::Thinking));
        let drawn = match in_row {
            InRow::Own(block) => recorded(TextAt { seq: toggle.seq, block }, &id, thought, None, toggle.find, shared),
            InRow::Call => {
                find::markdown_view(shared.texts.markdown(find::view_id(&id), thought), &id, toggle.find, None)
            }
        };
        text(toggle.key, drawn, cx).into_any_element()
    });
    line::drawn(line, None, opened, toggle, cx)
}

/// All of what the agent thought, `text_view`: its kit view, which is under its [`find::view_id`] while it streams and
/// once it's recorded in a call's row, or once it's recorded in a row of its own, drawn from the kit's blocks.
pub(super) fn text(key: &ItemKey, text_view: impl IntoElement, cx: &App) -> impl IntoElement {
    div().id(key.element("thinking")).mb_1().text_color(cx.theme().muted_foreground).child(text_view).test_support()
}
