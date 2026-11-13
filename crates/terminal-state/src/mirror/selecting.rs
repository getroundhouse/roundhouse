//! Selecting what a terminal shows with the pointer, as a terminal app does: a press and a drag select from cell to
//! cell, a second press selects the word there and a third the line, and a drag after either goes on by words or by
//! lines. Ghostty's gesture works that out from what each press selects by, and keeps the place the press was as the
//! program writes on. What it selects is the terminal's own selection, which Ghostty keeps on the same text as the
//! screen scrolls.

use libghostty_vt::Terminal;
use libghostty_vt::fmt::{Format, Formatter};
use libghostty_vt::grid_ref::GridRef;
use libghostty_vt::point::{Coordinate, Point};
use libghostty_vt::selection::gesture::{Behavior, Behaviors, DragEvent, Geometry, Gesture, PressEvent, ReleaseEvent};

use super::{Mirror, MirrorError};

#[cfg(test)]
mod tests;

/// Where the pointer is, in cells from the top left of what shows, with how far into its cell it is: half a column in
/// is 0.5. It's outside them while a drag goes past an edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pointer {
    pub col: f32,
    pub row: f32,
}

/// What a press selects, and a drag after it goes on selecting by: a first click's starts from its cell, a second's
/// selects the word there, and a third's the line, as the platform counts a run of clicks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectBy {
    Cells,
    Words,
    Lines,
}

/// How many of Ghostty's units of position a cell is, each way. Ghostty takes a cell's size in whole units, and a
/// cell is rarely a whole number of pixels, so the pointer is given in these rather than in pixels.
const CELL: u32 = 1000;

/// What a mirror keeps to select with, made the first time it's pressed on, since the worker's never is. The places
/// in the terminal its gesture keeps are the terminal's, which frees them only when it goes itself, so this lives and
/// goes with its mirror.
#[derive(Debug)]
pub(super) struct Selecting {
    gesture: Gesture<'static>,
    press: PressEvent<'static>,
    drag: DragEvent<'static>,
    release: ReleaseEvent<'static>,
}

impl Mirror {
    /// A press at `at`, which clears what was selected and starts from that cell, or selects the word or the line
    /// there.
    pub fn press(&mut self, at: Pointer, by: SelectBy) -> Result<(), MirrorError> {
        let Self { terminal, selecting, .. } = self;
        let selecting = match selecting {
            Some(selecting) => selecting,
            None => selecting.insert(Selecting {
                gesture: Gesture::new()?,
                press: PressEvent::new()?,
                drag: DragEvent::new()?,
                release: ReleaseEvent::new()?,
            }),
        };
        let behavior = match by {
            SelectBy::Cells => Behavior::Cell,
            SelectBy::Words => Behavior::Word,
            SelectBy::Lines => Behavior::Line,
        };
        let (x, y) = position(at);
        // With no time, Ghostty takes every press as a first click, and selects by what a first click is set to, so the
        // platform's count of the clicks is the only one.
        let behaviors = Behaviors::new().with_single_click_behavior(behavior);
        selecting.press.set_position(x, y)?.set_behaviors(&behaviors)?;
        let under = cell_under(terminal, at)?;
        let selection = selecting.press.apply(&mut selecting.gesture, terminal, under)?;
        terminal.set_selection(selection.as_ref())?;
        Ok(())
    }

    /// The pointer has moved to `at` with the button down since a press, which takes the selection there.
    pub fn drag(&mut self, at: Pointer) -> Result<(), MirrorError> {
        let Self { terminal, selecting, .. } = self;
        let Some(selecting) = selecting else { return Ok(()) };
        let (x, y) = position(at);
        selecting.drag.set_position(x, y)?;
        let geometry = Geometry {
            columns: u32::from(terminal.cols()?),
            cell_width: CELL,
            padding_left: 0,
            screen_height: u32::from(terminal.rows()?) * CELL,
        };
        let under = cell_under(terminal, at)?;
        let selection = selecting.drag.apply(&mut selecting.gesture, terminal, under, geometry)?;
        terminal.set_selection(selection.as_ref())?;
        Ok(())
    }

    /// The button has come up at `at`, which ends the drag and keeps what it selected.
    pub fn release(&mut self, at: Pointer) -> Result<(), MirrorError> {
        let Self { terminal, selecting, .. } = self;
        let Some(selecting) = selecting else { return Ok(()) };
        let under = cell_under(terminal, at)?;
        selecting.release.apply(&mut selecting.gesture, terminal, Some(under))?;
        Ok(())
    }

    /// Selects everything, the history included.
    pub fn select_all(&mut self) -> Result<(), MirrorError> {
        let all = self.terminal.select_all()?;
        self.terminal.set_selection(all.as_ref())?;
        Ok(())
    }

    pub fn clear_selection(&mut self) -> Result<(), MirrorError> {
        self.terminal.set_selection(None)?;
        Ok(())
    }

    /// What's selected, as text, with a line the terminal wrapped joined again, and the spaces at the end of a line
    /// left out, as Ghostty copies it. None while nothing is.
    pub fn selected_text(&self) -> Result<Option<String>, MirrorError> {
        let Some(mut formatter) = Formatter::new_selection(&self.terminal)? else { return Ok(None) };
        formatter.set_format(Format::Plain)?.set_unwrap(true)?.set_trim(true)?;
        Ok(Some(String::from_utf8_lossy(&formatter.format_alloc(None)?).into_owned()))
    }
}

/// The pointer in Ghostty's units, from the top left of the grid.
fn position(at: Pointer) -> (f64, f64) {
    (f64::from(at.col) * f64::from(CELL), f64::from(at.row) * f64::from(CELL))
}

/// The cell under the pointer, or the nearest one to it while it's past an edge.
fn cell_under<'t>(terminal: &'t Terminal<'_, '_>, at: Pointer) -> Result<GridRef<'t>, MirrorError> {
    let x = nearest(at.col, terminal.cols()?);
    let y = u32::from(nearest(at.row, terminal.rows()?));
    Ok(terminal.grid_ref(Point::Viewport(Coordinate { x, y }))?)
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to the cells there are first")]
fn nearest(at: f32, cells: u16) -> u16 {
    at.floor().clamp(0., f32::from(cells.saturating_sub(1))) as u16
}
