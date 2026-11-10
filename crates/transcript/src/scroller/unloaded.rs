//! Rows whose events haven't been fetched: drawn as tall as the rows that have been come to on average, and fetched
//! once one is on screen.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ops::Range;
use std::rc::Rc;

use client_core::OpenSession;
use gpui_kit::{AnyElement, IntoElement as _, ParentElement as _, Pixels, Styled as _, canvas, div, px};

/// What a row counts as before anything has been measured to go by. About what a session's rows come to on average,
/// most of them showing nothing and a few of them screens long.
const FIRST_GUESS: Pixels = px(48.);
/// How far the average can move before the rows drawn at the old one are drawn again.
const CLOSE_ENOUGH: f32 = 0.1;

#[derive(Clone)]
pub(crate) struct Unloaded(Rc<Inner>);

struct Inner {
    /// Each row drawn as unloaded, and how tall.
    drawn: RefCell<BTreeMap<usize, Pixels>>,
    guess: Cell<Pixels>,
}

impl Default for Unloaded {
    fn default() -> Self {
        Self(Rc::new(Inner { drawn: RefCell::default(), guess: Cell::new(FIRST_GUESS) }))
    }
}

impl Unloaded {
    /// Row `ix`, which hasn't been fetched, at the guess. It's fetched when it's drawn on screen, rather than when
    /// the list measures it, since the list measures every row.
    pub(crate) fn placeholder(&self, ix: usize, session: &OpenSession) -> AnyElement {
        let height = self.0.guess.get();
        self.0.drawn.borrow_mut().insert(ix, height);
        let session = session.clone();
        div()
            .w_full()
            .h(height)
            .child(canvas(move |_, _, _| session.want(ix), |_, (), _, _| {}).size_full())
            .into_any_element()
    }

    /// Row `ix` is drawn as what it is.
    pub(crate) fn loaded(&self, ix: usize) {
        self.0.drawn.borrow_mut().remove(&ix);
    }

    /// Rows only come and go at the end.
    pub(super) fn set_rows(&self, rows: usize) {
        self.0.drawn.borrow_mut().split_off(&rows);
    }

    /// Works the guess out again, from `height`, which the list measures all `rows` at, and returns the rows drawn
    /// at a guess that's now well off, to be drawn again.
    pub(super) fn reestimate(&self, rows: usize, height: Pixels) -> Vec<Range<usize>> {
        let drawn = self.0.drawn.borrow();
        let Some(loaded) = rows.checked_sub(drawn.len()).filter(|loaded| *loaded > 0) else { return Vec::new() };
        let unloaded_height = drawn.values().fold(px(0.), |sum, height| sum + *height);
        let average = (height - unloaded_height).max(px(0.)) / count(loaded);
        let guess = self.0.guess.get();
        if (average - guess).abs() <= guess.max(px(1.)) * CLOSE_ENOUGH {
            return Vec::new();
        }
        self.0.guess.set(average);
        let mut ranges: Vec<Range<usize>> = Vec::new();
        for (&row, _) in drawn.iter().filter(|(_, height)| (**height - average).abs() > px(0.5)) {
            match ranges.last_mut() {
                Some(range) if range.end == row => range.end += 1,
                Some(_) | None => ranges.push(row..row + 1),
            }
        }
        ranges
    }
}

#[expect(clippy::cast_precision_loss, reason = "a count of rows, which is exact in an f32 up to 16 million")]
fn count(rows: usize) -> f32 {
    rows as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawn(rows: &[(usize, f32)]) -> Unloaded {
        let unloaded = Unloaded::default();
        unloaded.0.drawn.borrow_mut().extend(rows.iter().map(|(row, height)| (*row, px(*height))));
        unloaded
    }

    #[test]
    fn the_guess_is_what_the_loaded_rows_come_to_on_average() {
        // Rows 0 to 3 aren't loaded, and the six that are come to 600 px between them.
        let unloaded = drawn(&[(0, 48.), (1, 48.), (2, 48.), (3, 48.)]);

        let redraw = unloaded.reestimate(10, px(600. + 4. * 48.));

        assert_eq!(unloaded.0.guess.get(), px(100.));
        assert_eq!(redraw, vec![0..4], "drawn again at it, as one run");
    }

    #[test]
    fn a_guess_close_enough_stays() {
        let unloaded = drawn(&[(0, 48.), (5, 48.)]);

        assert!(unloaded.reestimate(10, px(8. * 50. + 2. * 48.)).is_empty());
        assert_eq!(unloaded.0.guess.get(), FIRST_GUESS);
    }

    #[test]
    fn rows_drawn_apart_are_drawn_again_in_runs() {
        let unloaded = drawn(&[(0, 48.), (1, 48.), (7, 48.), (8, 20.)]);

        let redraw = unloaded.reestimate(20, px(16. * 20. + 3. * 48. + 20.));

        assert_eq!(unloaded.0.guess.get(), px(20.));
        assert_eq!(redraw, vec![0..2, 7..8], "a row already at the guess isn't");
    }

    #[test]
    fn with_nothing_loaded_there_s_nothing_to_go_by() {
        let unloaded = drawn(&[(0, 48.), (1, 48.)]);

        assert!(unloaded.reestimate(2, px(96.)).is_empty());
        assert_eq!(unloaded.0.guess.get(), FIRST_GUESS);
    }
}
