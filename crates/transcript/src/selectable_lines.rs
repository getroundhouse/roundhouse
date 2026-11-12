//! Lines of plain text with a little space between them, which select and copy as one text. The kit's
//! `SelectableText` is one text, laid out together, and texts split across several of them copy in no fixed order. So
//! each line is laid out on its own, and they're all reported to the window's selection as one participant's runs,
//! in order. A copy puts what's selected of each a line apart, as the text was. Each line draws its matches of what's
//! being found.

use std::rc::Rc;

use gpui_kit::base::{TextSelectionRun, Theme};
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Bounds, ElementId, InteractiveElement as _, IntoElement, ParentElement, Pixels, Point, RenderOnce,
    SharedString, Styled, StyledText, TestSupportExt as _, TextLayout, Window, div, fill,
};

use crate::find::Marks;
use crate::find::reveal::{self, Reveal};
use crate::window_selection::{Selectable, highlight};

/// Lines of text, `gap` apart, selectable as one.
#[derive(IntoElement)]
pub(crate) struct SelectableLines {
    id: ElementId,
    lines: Vec<SharedString>,
    gap: Pixels,
    /// The matches in the lines, a line apart, and the area they scroll in, which the match being shown says it was
    /// drawn in.
    marks: Option<(Marks, ElementId)>,
}

impl SelectableLines {
    /// `id` keeps what's selected from one frame to the next, so it's these lines' alone.
    pub(crate) fn new(id: impl Into<ElementId>, lines: Vec<SharedString>, gap: Pixels) -> Self {
        Self { id: id.into(), lines, gap, marks: None }
    }

    /// Draws `marks`, which are of the lines a line apart, scrolling in `area`.
    pub(crate) fn marked(mut self, marks: Marks, area: ElementId) -> Self {
        self.marks = (!marks.is_empty()).then_some((marks, area));
        self
    }
}

/// Each line's text, and how it was laid out.
type Runs = Rc<[(SharedString, TextLayout)]>;

impl RenderOnce for SelectableLines {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let selection = Selectable::kept(&self.id, window, cx);
        let (texts, here) = with_marks(&self.lines, self.marks.as_ref().map(|(marks, _)| marks));
        let runs: Runs = self.lines.into_iter().zip(texts.iter().map(|text| text.layout().clone())).collect();
        let here = here.zip(self.marks.map(|(_, area)| area));
        let texts: Vec<AnyElement> = texts
            .into_iter()
            .enumerate()
            .map(|(ix, text)| match &here {
                Some(((line, start, mark), area)) if *line == ix => {
                    let layout = text.layout().clone();
                    let mark = reveal::here(layout, *start, Some(area.clone()), mark.clone());
                    div().relative().child(text).child(mark).into_any_element()
                }
                Some(_) | None => text.into_any_element(),
            })
            .collect();
        let gap = self.gap;
        div()
            .id(self.id)
            .relative()
            .child(selection.under(move |selection, window, cx| paint(selection, &runs, gap, window, cx)))
            .child(v_flex().gap(gap).children(texts))
            .test_support()
    }
}

/// Where the match being shown starts: the line it's in, how far into it, and what it says where it was drawn.
type Here = (usize, usize, Reveal);

/// Each line laid out with its matches, and where the match being shown starts, if it's in one.
fn with_marks(lines: &[SharedString], marks: Option<&Marks>) -> (Vec<StyledText>, Option<Here>) {
    let Some(marks) = marks else { return (lines.iter().cloned().map(StyledText::new).collect(), None) };
    let (mut at, mut here) = (0, None);
    let texts = lines
        .iter()
        .enumerate()
        .map(|(ix, line)| {
            let own = marks.within(at, at + line.len());
            at += line.len() + 1;
            if here.is_none()
                && let Some((range, mark)) = own.here().zip(own.reveal())
            {
                here = Some((ix, range.start, mark.clone()));
            }
            StyledText::new(line.clone()).with_highlights(own.over(&[], line.len()))
        })
        .collect();
    (texts, here)
}

/// Paints what's selected of each line, and keeps it, a line apart, for a copy. A selection that goes on into the
/// next line reaches the edge and fills the space between them, so it reads as one.
fn paint(selection: &Selectable, runs: &Runs, gap: Pixels, window: &mut Window, cx: &mut App) {
    let reported: Vec<TextSelectionRun> = (0_u64..)
        .zip(runs.iter())
        .map(|(order, (text, layout))| {
            TextSelectionRun::new(text.clone(), layout.clone(), layout.bounds()).with_document_order(order)
        })
        .collect();
    let projection = selection.project(&reported, cx);
    let ranges = projection.ranges();
    let color = Theme::global(cx).tokens.colors.selection;
    let mut selected = Vec::new();
    for (ix, ((text, layout), range)) in runs.iter().zip(ranges).enumerate() {
        let Some(range) = range.clone() else { continue };
        selected.push(&text[range.clone()]);
        let (Some(start), Some(end)) = (layout.position_for_index(range.start), layout.position_for_index(range.end))
        else {
            continue;
        };
        let line = layout.bounds();
        let goes_on = ranges.get(ix + 1).is_some_and(Option::is_some);
        let end = if goes_on { Point::new(line.right(), end.y) } else { end };
        for bounds in highlight(start, end, line, layout.line_height()) {
            window.paint_quad(fill(bounds, color));
        }
        if goes_on {
            let between = Bounds::from_corners(line.bottom_left(), Point::new(line.right(), line.bottom() + gap));
            window.paint_quad(fill(between, color));
        }
    }
    selection.keep(selected.join("\n"));
}
