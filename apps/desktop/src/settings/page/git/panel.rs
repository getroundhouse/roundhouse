//! A panel of Git's, framed as [`panel::framed`] frames any page's, and placed by what the page keeps.

use gpui_kit::{AnyElement, App, IntoElement, Window};

use super::super::panel;
pub(super) use super::super::panel::{Closes, Framed, button, buttons, choice, choices, waiting_line};
use super::Drawing;
use super::kept::Panel;

/// The page's name, which its panels' parts are known by.
const PAGE: &str = "git";

/// `open`, as `framed` says.
pub(super) fn framed(drawing: &Drawing<'_>, open: &Panel, framed: Framed, cx: &App) -> AnyElement {
    let (kept, scroll) = (drawing.kept.clone(), drawing.scroll.clone());
    let laid_out = move |bounds, window: &mut Window, cx: &mut App| {
        kept.update(cx, |kept, _| kept.laid_out(bounds, &scroll, window));
    };
    panel::framed(PAGE, &open.place, framed, laid_out, drawing.open_panel, cx)
}

/// Why the hub, or GitHub, refused what a panel asked.
pub(super) fn refusal(refused: String, cx: &App) -> impl IntoElement + use<> {
    panel::refusal(PAGE, refused, cx)
}
