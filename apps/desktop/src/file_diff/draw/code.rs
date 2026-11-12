//! A line's code as it's drawn: in its colours, with what changed within it tinted under them, and what's selected of
//! it under that, scrolled sideways as far as the diff is. Each line drawn is noted, so the pointer can be found in it.

use std::ops::Range;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyElement, App, HighlightStyle, Hsla, IntoElement as _, ParentElement as _, Styled as _, StyledText, div,
};

use super::Drawing;
use crate::file_diff::selection::Drawn;
use crate::file_view::Line;

/// A line's code, and where it is.
pub(super) struct Code<'a> {
    pub(super) row: usize,
    /// Whether it's the old side's.
    pub(super) old: bool,
    pub(super) line: Option<&'a Line>,
    /// What changed within it.
    pub(super) changed: &'a [Range<usize>],
}

/// `code`, with what changed in it in `tint`, in a box as wide as there's room for, which it's scrolled sideways in.
pub(super) fn code(code: &Code<'_>, tint: Option<Hsla>, drawing: &Drawing, cx: &App) -> AnyElement {
    let text = code.line.map(|line| {
        let selected = drawing.selection.and_then(|selection| selection.within(code.row, code.old, line.text.len()));
        let styles = styled(line, code.changed, tint, selected, cx.theme().selection);
        let text = StyledText::new(line.text.clone()).with_highlights(styles);
        let layout = text.layout().clone();
        drawing.drawn.borrow_mut().push(Drawn { row: code.row, old: code.old, layout });
        text
    });
    div()
        .flex_1()
        .min_w_0()
        .h_full()
        .overflow_hidden()
        .child(div().absolute().top_0().left(-drawing.across).whitespace_nowrap().children(text))
        .into_any_element()
}

/// The line's syntax colours, with what changed within it in `tint` under them, and what's `selected` of it in the
/// selection's colour, over the tint.
fn styled(
    line: &Line,
    changed: &[Range<usize>],
    tint: Option<Hsla>,
    selected: Option<Range<usize>>,
    selection: Hsla,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let changed = if tint.is_some() { changed } else { &[] };
    let selected = selected.filter(|range| !range.is_empty());
    if changed.is_empty() && selected.is_none() {
        return line.styles.clone();
    }
    let ends = line.styles.iter().map(|(range, _)| range).chain(changed).chain(&selected);
    let mut cuts: Vec<usize> = ends.flat_map(|range| [range.start, range.end]).collect();
    cuts.sort_unstable();
    cuts.dedup();
    let within = |range: &Range<usize>, piece: &Range<usize>| range.start <= piece.start && piece.end <= range.end;
    cuts.windows(2)
        .filter_map(|ends| {
            let piece = ends[0]..ends[1];
            let syntax = line.styles.iter().find(|(range, _)| within(range, &piece)).map(|(_, style)| *style);
            let tinted = changed.iter().any(|range| within(range, &piece)).then_some(tint).flatten();
            let background = selected.as_ref().filter(|range| within(range, &piece)).map(|_| selection).or(tinted);
            let style = HighlightStyle { background_color: background, ..syntax.unwrap_or_default() };
            (syntax.is_some() || background.is_some()).then_some((piece, style))
        })
        .collect()
}
