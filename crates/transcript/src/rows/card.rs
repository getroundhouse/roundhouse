//! The look of a card in the chat, which stands apart from the rows around it.

use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{App, Div, Hsla, Styled};

/// A card, with an edge and a faint wash of `tint` when it has one, as an approval's has amber while it waits on the
/// person and a failed request's has red, and the plain edge otherwise.
pub(super) fn tinted(tint: Option<Hsla>, cx: &App) -> Div {
    v_flex()
        .w_full()
        .p_3()
        .rounded_lg()
        .border_1()
        .border_color(tint.map_or(look::colors(cx).edge, |tint| tint.opacity(0.45)))
        .when_some(tint, |d, tint| d.bg(tint.opacity(0.05)))
}
