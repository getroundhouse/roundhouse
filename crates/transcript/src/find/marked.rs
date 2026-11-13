//! Drawing a text with its matches behind it.

use std::ops::Range;

use gpui_kit::{
    AnyElement, ElementId, HighlightStyle, IntoElement, ParentElement as _, SharedString, Styled as _, StyledText,
    TextLayout, div,
};

use super::Marks;
use super::reveal::{self, Reveal};

/// `text` with `styles`, such as code's colours, and its matches behind it. The match being shown says where it was
/// drawn, in `area` if it's in an area that scrolls.
pub(crate) fn marked(
    text: SharedString,
    marks: &Marks,
    styles: &[(Range<usize>, HighlightStyle)],
    area: Option<ElementId>,
) -> AnyElement {
    marked_with_layout(text, marks, styles, area).0
}

/// What [`marked`] draws, and how its text is laid out there, for a text that's selected, which is painted by where
/// its characters are.
pub(crate) fn marked_with_layout(
    text: SharedString,
    marks: &Marks,
    styles: &[(Range<usize>, HighlightStyle)],
    area: Option<ElementId>,
) -> (AnyElement, TextLayout) {
    let len = text.len();
    let here = marks.here().zip(marks.reveal()).map(|(range, reveal)| (range.start, reveal.clone()));
    let shown = StyledText::new(text).with_highlights(marks.over(styles, len));
    let layout = shown.layout().clone();
    (with_here(shown, here, area), layout)
}

/// `text` with `styles`, which have the matches' backgrounds in them already. The match being shown, if it's here,
/// starts at the offset `here` has, and says where it was drawn, in `area` if it's in an area that scrolls.
pub(crate) fn styled(
    text: SharedString,
    styles: Vec<(Range<usize>, HighlightStyle)>,
    here: Option<(usize, Reveal)>,
    area: Option<ElementId>,
) -> AnyElement {
    with_here(StyledText::new(text).with_highlights(styles), here, area)
}

/// `shown`, with the match being shown, if it's here, saying where it was drawn.
fn with_here(shown: StyledText, here: Option<(usize, Reveal)>, area: Option<ElementId>) -> AnyElement {
    let Some((start, reveal)) = here else { return shown.into_any_element() };
    let layout = shown.layout().clone();
    div().relative().child(shown).child(reveal::here(layout, start, area, reveal)).into_any_element()
}
