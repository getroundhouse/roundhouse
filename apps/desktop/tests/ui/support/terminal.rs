//! Pointing at a terminal's cells, as a hand points at what it shows.

#![cfg(test)]

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Pixels, Point, Size, Window, point};

/// A place in a terminal so many cells along a row, as 2.5 is halfway through the third cell, halfway down the row.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Cells {
    pub(crate) along: f32,
    pub(crate) row: u16,
}

/// Where `at` is in the window, in the terminal showing, whose cells are `cell` big, from the grid's top left, which
/// is its view's.
pub(crate) fn in_window(window: &mut Window, cell: Size<Pixels>, at: Cells) -> Point<Pixels> {
    let origin = window.find(terminal_view::SHOWN_ID).bounds().origin;
    origin + point(cell.width * at.along, cell.height * (f32::from(at.row) + 0.5))
}
