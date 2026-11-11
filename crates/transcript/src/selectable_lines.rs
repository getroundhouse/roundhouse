//! Lines of plain text with a little space between them, which select and copy as one text. The kit's
//! `SelectableText` is one text, laid out together, and texts split across several of them copy in no fixed order. So
//! each line is laid out on its own, and they're all reported to the window's selection as one participant's runs,
//! in order. A copy puts what's selected of each a line apart, as the text was.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::base::{TextSelectionHandle, TextSelectionRegistration, TextSelectionRun, Theme};
use gpui_kit::component::v_flex;
use gpui_kit::{
    App, Bounds, ElementId, HitboxBehavior, InteractiveElement as _, IntoElement, ParentElement, Pixels, Point,
    RenderOnce, SharedString, Styled, StyledText, TestSupportExt as _, TextLayout, Window, canvas, div, fill,
};

/// Lines of text, `gap` apart, selectable as one.
#[derive(IntoElement)]
pub(crate) struct SelectableLines {
    id: ElementId,
    lines: Vec<SharedString>,
    gap: Pixels,
}

impl SelectableLines {
    /// `id` keeps what's selected from one frame to the next, so it's these lines' alone.
    pub(crate) fn new(id: impl Into<ElementId>, lines: Vec<SharedString>, gap: Pixels) -> Self {
        Self { id: id.into(), lines, gap }
    }
}

/// What the lines keep from frame to frame: their part in the window's selection, and what's selected of them, for
/// a copy to take.
struct Kept {
    handle: TextSelectionHandle,
    selected: Rc<RefCell<String>>,
}

/// Each line's text, and how it was laid out.
type Runs = Rc<[(SharedString, TextLayout)]>;

impl RenderOnce for SelectableLines {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let kept = window.use_keyed_state((self.id.clone(), "selection"), cx, |_, cx| {
            let (handle, selected) =
                (TextSelectionHandle::new(String::new(), cx), Rc::new(RefCell::new(String::new())));
            let copied = selected.clone();
            handle.copy_with(move |_| copied.borrow().clone(), cx);
            Kept { handle, selected }
        });
        let (handle, selected) = {
            let kept = kept.read(cx);
            (kept.handle.clone(), kept.selected.clone())
        };
        let texts: Vec<StyledText> = self.lines.iter().cloned().map(StyledText::new).collect();
        let runs: Runs = self.lines.into_iter().zip(texts.iter().map(|text| text.layout().clone())).collect();
        let gap = self.gap;
        // First, so the selection is painted under the text. Painting comes after everything is laid out and placed,
        // so the lines are where they're drawn by then.
        let selection = canvas(
            {
                let handle = handle.clone();
                move |bounds, window, cx| report(&handle, bounds, window, cx)
            },
            move |_, (), window, cx| paint(&handle, &runs, gap, &selected, window, cx),
        );
        div()
            .id(self.id)
            .relative()
            .child(div().absolute().inset_0().child(selection.size_full()))
            .child(v_flex().gap(gap).children(texts))
            .test_support()
    }
}

/// Tells the window's selection where the lines are, so a drag that starts anywhere over them selects them, as one
/// over the kit's own text does.
fn report(handle: &TextSelectionHandle, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
    let registration = TextSelectionRegistration::new(hitbox, bounds)
        .with_text_bounds(vec![bounds])
        .with_rendered_element(handle, window, cx);
    handle.register(registration, window, cx);
}

/// Paints what's selected of each line, and keeps it, a line apart, for a copy. A selection that goes on into the
/// next line reaches the edge and fills the space between them, so it reads as one.
fn paint(
    handle: &TextSelectionHandle,
    runs: &Runs,
    gap: Pixels,
    kept: &RefCell<String>,
    window: &mut Window,
    cx: &mut App,
) {
    let reported: Vec<TextSelectionRun> = (0_u64..)
        .zip(runs.iter())
        .map(|(order, (text, layout))| {
            TextSelectionRun::new(text.clone(), layout.clone(), layout.bounds()).with_document_order(order)
        })
        .collect();
    let projection = handle.update_runs(&reported, cx);
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
    let selected = selected.join("\n");
    // What's selected has changed since the last frame, so whatever shows it draws again, as the kit's text does.
    if *kept.borrow() != selected {
        *kept.borrow_mut() = selected;
        window.refresh();
    }
}

/// Where the selection from `start` to `end` in a line lies: across its rows, to the line's edges between them.
fn highlight(start: Point<Pixels>, end: Point<Pixels>, line: Bounds<Pixels>, row: Pixels) -> Vec<Bounds<Pixels>> {
    if start.y == end.y {
        return vec![Bounds::from_corners(start, Point::new(end.x, end.y + row))];
    }
    let first = Bounds::from_corners(start, Point::new(line.right(), start.y + row));
    let between = Bounds::from_corners(Point::new(line.left(), start.y + row), Point::new(line.right(), end.y));
    let last = Bounds::from_corners(Point::new(line.left(), end.y), Point::new(end.x, end.y + row));
    [first, between, last].into_iter().filter(|bounds| bounds.size.height > Pixels::ZERO).collect()
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, Point, point, px};

    use super::highlight;

    #[test]
    fn a_selection_within_a_row_is_one_box_and_across_rows_reaches_the_line_s_edges() {
        let line = Bounds::from_corners(point(px(0.), px(0.)), point(px(100.), px(60.)));

        assert_eq!(highlight(point(px(10.), px(0.)), point(px(40.), px(0.)), line, px(20.)).len(), 1);

        let across = highlight(point(px(10.), px(0.)), point(px(40.), px(40.)), line, px(20.));
        assert_eq!(across.len(), 3);
        assert_eq!(across[0].right(), px(100.), "the first row to the line's right edge");
        assert_eq!(across[2].origin, Point::new(px(0.), px(40.)), "the last from its left edge");

        let next = highlight(point(px(10.), px(0.)), point(px(40.), px(20.)), line, px(20.));
        assert_eq!(next.len(), 2, "with no rows between, there's nothing between to fill");
    }
}
