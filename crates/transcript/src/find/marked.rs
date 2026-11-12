//! Drawing a text with its matches behind it.

use std::ops::Range;

use gpui_kit::component::text::TextView;
use gpui_kit::{
    AnyElement, ElementId, HighlightStyle, IntoElement, ParentElement as _, SharedString, Styled as _, StyledText, div,
};

use super::reveal::{self, Reveal};
use super::{FindLook, Marks, TextId};

/// `text` with `styles`, such as code's colours, and its matches behind it. The match being shown says where it was
/// drawn, in `area` if it's in an area that scrolls.
pub(crate) fn marked(
    text: SharedString,
    marks: &Marks,
    styles: &[(Range<usize>, HighlightStyle)],
    area: Option<ElementId>,
) -> AnyElement {
    let len = text.len();
    let here = marks.here().zip(marks.reveal()).map(|(range, reveal)| (range.start, reveal.clone()));
    styled(text, marks.over(styles, len), here, area)
}

/// `text` with `styles`, which have the matches' backgrounds in them already. The match being shown, if it's here,
/// starts at the offset `here` has, and says where it was drawn, in `area` if it's in an area that scrolls.
pub(crate) fn styled(
    text: SharedString,
    styles: Vec<(Range<usize>, HighlightStyle)>,
    here: Option<(usize, Reveal)>,
    area: Option<ElementId>,
) -> AnyElement {
    let shown = StyledText::new(text).with_highlights(styles);
    let Some((start, reveal)) = here else { return shown.into_any_element() };
    let layout = shown.layout().clone();
    div().relative().child(shown).child(reveal::here(layout, start, area, reveal)).into_any_element()
}

/// The kit's view of the Markdown text `id`, which draws its own matches, as `crate::long_texts` has it do, and brings
/// the match being shown into view itself when it's in it, in `area` if it's in an area that scrolls.
pub(crate) fn markdown_view(view: TextView, id: &TextId, find: &FindLook, area: Option<ElementId>) -> AnyElement {
    if find.nth_here(id).is_some() { reveal::markdown(view, area, find.reveal()) } else { view.into_any_element() }
}
