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

/// Lays out each row of `cells` side by side, the first of each 60 wide and the second 140, one row under another, in
/// a window 200 wide, as a table's cells are, and gives each cell's text with its layout.
pub(super) fn side_by_side(
    window: &mut VisualTestContext,
    cells: &[[&'static str; 2]],
) -> Vec<[(&'static str, TextLayout); 2]> {
    let shown: Vec<[StyledText; 2]> = cells.iter().map(|row| row.map(StyledText::new)).collect();
    let layouts = (cells.iter().zip(&shown))
        .map(|(row, texts)| [0, 1].map(|cell| (row[cell], texts[cell].layout().clone())))
        .collect();
    let rows = shown.into_iter().map(|[name, said]| {
        div().flex().flex_row().child(div().w(px(60.)).child(name)).child(div().w(px(140.)).child(said))
    });
    let table = div().w(px(200.)).flex().flex_col().children(rows);
    window.draw(point(px(0.), px(0.)), size(px(200.), px(200.)), |_, _| table.into_element());
    layouts
}

/// Lays out each of `first` and `second` as texts of their own, one under another, 4 apart in `first` and 12 in
/// `second`, which starts `second_top` down from the top of a window 200 wide, whatever's above, and gives their
/// layouts, as an area's text and one below it are laid out in a chat.
pub(super) fn one_above_another(
    window: &mut VisualTestContext,
    first: &[&'static str],
    second: &[&'static str],
    second_top: f32,
) -> (Vec<TextLayout>, Vec<TextLayout>) {
    let shown = |texts: &[&'static str]| texts.iter().map(|text| StyledText::new(*text)).collect::<Vec<_>>();
    let (first, second) = (shown(first), shown(second));
    let layouts = |texts: &[StyledText]| texts.iter().map(|text| text.layout().clone()).collect();
    let laid_out = (layouts(&first), layouts(&second));
    let column =
        |top: f32, gap: f32| div().absolute().top(px(top)).left(px(0.)).w(px(200.)).flex().flex_col().gap(px(gap));
    let both = div()
        .relative()
        .size_full()
        .child(column(0., 4.).children(first))
        .child(column(second_top, 12.).children(second));
    window.draw(point(px(0.), px(0.)), size(px(200.), px(2000.)), |_, _| both.into_element());
    laid_out
}
