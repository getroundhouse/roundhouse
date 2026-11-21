//! What Ghostty selects for a double or a triple click, and copies of what's selected, which only tests use. A client
//! shows the ends the worker finds and copies the text it gives, so they're checked against what a click in Ghostty
//! itself would select and copy.

use libghostty_vt::fmt::{Format, Formatter};
use libghostty_vt::point::{Coordinate, Point};
use libghostty_vt::selection::gesture::{Behavior, Behaviors, Gesture, PressEvent};

use super::Mirror;

/// How many of Ghostty's units of position a cell is, each way, which its gesture takes a cell's size in.
const CELL: f64 = 1000.;

impl Mirror {
    /// Selects as a click at the middle of the cell at `col` and `row` of the screen does, as Ghostty's gesture takes
    /// it, a double click selecting by words and a triple by lines.
    pub(in crate::mirror) fn click(&mut self, col: u16, row: u16, by: Behavior) {
        let mut gesture = Gesture::new().unwrap();
        let mut press = PressEvent::new().unwrap();
        // With no time, Ghostty takes every press as a first click, and selects by what a first click is set to.
        let behaviors = Behaviors::new().with_single_click_behavior(by);
        let (x, y) = (f64::from(col) + 0.5, f64::from(row) + 0.5);
        press.set_position(x * CELL, y * CELL).unwrap().set_behaviors(&behaviors).unwrap();
        let under = self.terminal.grid_ref(Point::Viewport(Coordinate { x: col, y: u32::from(row) })).unwrap();
        let selection = press.apply(&mut gesture, &self.terminal, under).unwrap();
        self.terminal.set_selection(selection.as_ref()).unwrap();
    }

    /// Selects everything, the history included.
    pub(in crate::mirror) fn select_all(&mut self) {
        let all = self.terminal.select_all().unwrap();
        self.terminal.set_selection(all.as_ref()).unwrap();
    }

    pub(in crate::mirror) fn clear_selection(&mut self) {
        self.terminal.set_selection(None).unwrap();
    }

    /// What's selected, as Ghostty copies it, with a line the terminal wrapped joined again, and the spaces at the end
    /// of a line left out. None while nothing is.
    pub(in crate::mirror) fn selected_text(&self) -> Option<String> {
        let mut formatter = Formatter::new_selection(&self.terminal).unwrap()?;
        self.history_read();
        formatter.set_format(Format::Plain).unwrap().set_unwrap(true).unwrap().set_trim(true).unwrap();
        Some(String::from_utf8_lossy(&formatter.format_alloc(None).unwrap()).into_owned())
    }
}
