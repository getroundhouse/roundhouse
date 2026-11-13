//! Texts laid out by GPUI for the selection's tests, to find places in and paint what's selected of.

use gpui_kit::{
    IntoElement as _, ParentElement as _, Styled as _, StyledText, TextLayout, VisualTestContext, div, point, px, size,
};

use super::Pieces;

/// Lays out each of `texts` as a text of its own, one under another, `gap` apart, in a window 200 wide, and gives
/// their layouts.
pub(super) fn laid_out(window: &mut VisualTestContext, texts: &[&'static str], gap: f32) -> Vec<TextLayout> {
    let shown: Vec<StyledText> = texts.iter().map(|text| StyledText::new(*text)).collect();
    let layouts = shown.iter().map(|text| text.layout().clone()).collect();
    let column = div().w(px(200.)).flex().flex_col().gap(px(gap)).children(shown);
    window.draw(point(px(0.), px(0.)), size(px(200.), px(200.)), |_, _| column.into_element());
    layouts
}

/// Lays out `texts` as [`laid_out`] does, as the pieces of one text, a line break apart, and gives the pieces and the
/// layout of each.
pub(super) fn in_pieces(window: &mut VisualTestContext, texts: &[&'static str], gap: f32) -> (Pieces, Vec<TextLayout>) {
    let layouts = laid_out(window, texts, gap);
    let mut each = layouts.iter().cloned();
    let pieces = Pieces::lines(texts.iter().copied(), |_, _| each.next().expect("a layout for each piece"));
    (pieces, layouts)
}
