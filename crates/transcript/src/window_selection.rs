//! How the transcript's texts that the window's selection holds, rather than its own in `crate::selection`, such as
//! what a command printed, take part in it. Each draws the window again whenever what's selected of it changes, as a
//! drag goes over it. Nothing else would until the drag ends, since what a drag changes is the selection, and nothing
//! that's drawn. The texts the transcript lays out itself share their part in the selection, kept from one frame to
//! the next, and a layer under the text that tells the selection where the text is and paints what's selected of it.
//! The rest are the kit's own selectable text.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::base::{self, TextSelectionHandle, TextSelectionProjection, TextSelectionRegistration, TextSelectionRun};
use gpui_kit::{
    App, Bounds, ElementId, HitboxBehavior, IntoElement, ParentElement as _, Pixels, Point, RenderOnce, SharedString,
    Styled as _, Subscription, Window, canvas, div,
};

/// A text's part in the window's selection, and what a copy takes of what's selected of it.
#[derive(Clone)]
pub(crate) struct Selectable {
    handle: TextSelectionHandle,
    copied: Rc<RefCell<String>>,
    _redraws: Rc<Subscription>,
}

impl Selectable {
    /// The selection kept under `id`, which is the text's alone.
    pub(crate) fn kept(id: &ElementId, window: &mut Window, cx: &mut App) -> Self {
        let kept = window.use_keyed_state((id.clone(), "selection"), cx, |window, cx| {
            let (handle, copied) = (TextSelectionHandle::new(String::new(), cx), Rc::new(RefCell::new(String::new())));
            let taken = copied.clone();
            handle.copy_with(move |_| taken.borrow().clone(), cx);
            let redraws = redraws(&handle, window, cx);
            Self { handle, copied, _redraws: redraws }
        });
        kept.read(cx).clone()
    }

    /// A layer that fills what holds the text, which is `relative` so the layer can, and goes first in it so it's
    /// painted under the text. It tells the
    /// window's selection where the text is, so a drag that starts anywhere over it selects it, as one over the kit's
    /// own text does, and then `paint` paints what's selected. Painting comes after everything is laid out and
    /// placed, so the text is where it's drawn by then.
    pub(crate) fn under(self, paint: impl FnOnce(&Self, &mut Window, &mut App) + 'static) -> impl IntoElement {
        let handle = self.handle.clone();
        let layer = canvas(
            move |bounds, window, cx| report(&handle, bounds, window, cx),
            move |_, (), window, cx| paint(&self, window, cx),
        );
        div().absolute().inset_0().child(layer.size_full())
    }

    /// What's selected of each of the text's `runs`.
    pub(crate) fn project(&self, runs: &[TextSelectionRun], cx: &mut App) -> TextSelectionProjection {
        self.handle.update_runs(runs, cx)
    }

    /// Keeps `copied` for a copy to take.
    pub(crate) fn keep(&self, copied: String) {
        *self.copied.borrow_mut() = copied;
    }
}

/// The kit's selectable text, drawn again as a drag goes over it.
#[derive(IntoElement)]
pub(crate) struct SelectableText {
    /// Keeps its part in the selection from one frame to the next, so it's this text's alone.
    id: ElementId,
    text: SharedString,
}

impl SelectableText {
    pub(crate) fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self { id: id.into(), text: text.into() }
    }
}

impl RenderOnce for SelectableText {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let text = self.text.clone();
        let kept = window.use_keyed_state((self.id.clone(), "selection"), cx, |window, cx| {
            let handle = TextSelectionHandle::new(text, cx);
            let redraws = redraws(&handle, window, cx);
            (handle, redraws)
        });
        let handle = kept.read(cx).0.clone();
        base::SelectableText::with_handle(self.id, handle, self.text)
    }
}

/// Draws the window again whenever what's selected of `handle`'s text changes, for as long as it's kept.
fn redraws(handle: &TextSelectionHandle, window: &Window, cx: &mut App) -> Rc<Subscription> {
    Rc::new(handle.refresh_window_on_change(window, cx))
}

/// Tells the window's selection that the text is within `bounds`.
fn report(handle: &TextSelectionHandle, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
    let registration = TextSelectionRegistration::new(hitbox, bounds)
        .with_text_bounds(vec![bounds])
        .with_rendered_element(handle, window, cx);
    handle.register(registration, window, cx);
}

/// Where the selection from `start` to `end` in a line lies: across its rows, to the line's edges between them.
pub(crate) fn highlight(
    start: Point<Pixels>,
    end: Point<Pixels>,
    line: Bounds<Pixels>,
    row: Pixels,
) -> Vec<Bounds<Pixels>> {
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
