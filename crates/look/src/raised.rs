//! A raised surface: something that sits over the pane, such as a card or what a hover shows.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{App, Div, Styled as _, div, px};

/// A card's corners, as `docs/spec/look.md` gives them.
const CORNER: f32 = 10.;

/// An empty raised surface, in the raised colour with the one thin edge and a card's corners, and no shadow. Whoever
/// fills it says how wide it is and how much room its content has.
pub fn raised(cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .rounded(px(CORNER))
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
}
