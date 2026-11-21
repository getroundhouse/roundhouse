//! What's selected in a terminal, by row number, and how the pointer selects, as Ghostty's selection gesture does: a
//! drag from cell to cell, taking in a cell at either end only once it covers six tenths of it, and a word's or a
//! line's ends, which the worker finds, for a double or a triple click and a drag after one.

use protocol::terminal::{Bounds, RowNumber, TerminalPoint, TerminalRange};
use terminal_state::SelectBy;

/// How far across its cell a drag has to cover a cell at either end of it before it's taken in, in thousandths, as in
/// Ghostty.
const TAKEN_IN_FROM: u32 = 600;

/// Which press or drag asked for a word's or a line's ends, so an answer to one that a later one has passed is dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::client::open_terminal) struct Step(u64);

/// A press of the pointer's button, and what it selected.
#[derive(Debug)]
struct Press {
    at: TerminalPoint,
    /// How far across its cell it was, in thousandths.
    across: u32,
    by: SelectBy,
    step: Step,
    /// The word or the line pressed on, once its ends have come.
    pressed: Option<TerminalRange>,
    /// The newest drag that asked for the ends at it, and where it was.
    dragged: Option<(Step, TerminalPoint)>,
    /// Whether the button is still down.
    down: bool,
}

#[derive(Debug, Default)]
pub(super) struct Selecting {
    /// Its two ends, the one the press began from first, which may come after the other.
    selected: Option<(TerminalPoint, TerminalPoint)>,
    press: Option<Press>,
    steps: u64,
}

impl Selecting {
    /// Its cells, in reading order.
    pub(super) fn range(&self) -> Option<TerminalRange> {
        let (start, end) = self.selected?;
        let (from, to) = if end < start { (end, start) } else { (start, end) };
        Some(TerminalRange { from, to })
    }

    /// The first and last of its columns on row `number`, of a row `cols` wide.
    pub(super) fn cells(&self, number: RowNumber, cols: u16) -> Option<(u16, u16)> {
        let TerminalRange { from, to } = self.range()?;
        if number < from.row || number > to.row {
            return None;
        }
        let first = if number == from.row { from.col } else { 0 };
        let last = if number == to.row { to.col } else { cols.saturating_sub(1) };
        Some((first, last))
    }

    pub(super) fn clear(&mut self) {
        self.selected = None;
        self.press = None;
    }

    pub(super) fn all(&mut self, range: TerminalRange) {
        self.press = None;
        self.selected = Some((range.from, range.to));
    }

    /// Its ends where a new size put them, and none if either has gone. A drag under way stops, keeping what it had.
    pub(super) fn moved(&mut self, from: Option<TerminalPoint>, to: Option<TerminalPoint>) {
        self.press = None;
        self.selected = from.zip(to);
    }

    fn step(&mut self) -> Step {
        self.steps += 1;
        Step(self.steps)
    }

    /// A press at `at`, which clears what was selected, and for a word or a line asks for its ends.
    pub(super) fn press(&mut self, at: TerminalPoint, across: u32, by: SelectBy) -> Option<(Bounds, Step)> {
        let step = self.step();
        self.selected = None;
        self.press = Some(Press { at, across, by, step, pressed: None, dragged: None, down: true });
        match by {
            SelectBy::Cells => None,
            SelectBy::Words => Some((Bounds::Word, step)),
            SelectBy::Lines => Some((Bounds::Line, step)),
        }
    }

    /// The pointer has moved to `at` with the button down since a press, in a terminal `cols` wide whose screen ends
    /// before `end`. A drag by words or lines asks for the ends at `at`.
    pub(super) fn drag(&mut self, at: TerminalPoint, across: u32, cols: u16, end: RowNumber) -> Option<(Bounds, Step)> {
        let step = self.step();
        let press = self.press.as_mut().filter(|press| press.down)?;
        match press.by {
            SelectBy::Cells => {
                self.selected = by_cells(press, at, across, cols, end);
                None
            }
            SelectBy::Words | SelectBy::Lines => {
                press.dragged = Some((step, at));
                Some((if press.by == SelectBy::Words { Bounds::Word } else { Bounds::Line }, step))
            }
        }
    }

    pub(super) fn release(&mut self) {
        if let Some(press) = &mut self.press {
            press.down = false;
        }
    }

    /// The ends of the word or the line a press or a drag asked for. Returns whether what's selected changed.
    pub(super) fn bounds(&mut self, step: Step, range: Option<TerminalRange>) -> bool {
        let Some(press) = &mut self.press else { return false };
        if step == press.step {
            press.pressed = range;
            self.selected = range.map(|range| (range.from, range.to));
            return true;
        }
        let (Some((dragged, at)), Some(pressed), Some(current)) = (press.dragged, press.pressed, range) else {
            return false;
        };
        if dragged != step {
            return false;
        }
        // Ghostty's word or line drag: from the one pressed on to the one under the pointer, whichever way it went.
        self.selected = Some(if at < press.at { (pressed.to, current.from) } else { (pressed.from, current.to) });
        true
    }
}

/// What a drag by cells from `press` to `at` selects, as Ghostty's `dragSelection` has it: the cell pressed on and the
/// cell under the pointer are each taken in only if the drag covers six tenths of it, the way it went.
fn by_cells(
    press: &Press,
    at: TerminalPoint,
    across: u32,
    cols: u16,
    end: RowNumber,
) -> Option<(TerminalPoint, TerminalPoint)> {
    let same = at == press.at;
    let backwards = if same { across < press.across } else { at < press.at };
    let take_pressed = if backwards { press.across >= TAKEN_IN_FROM } else { press.across < TAKEN_IN_FROM };
    let take_under = if backwards { across < TAKEN_IN_FROM } else { across >= TAKEN_IN_FROM };
    let start = match (take_pressed, backwards) {
        (true, _) => press.at,
        (false, true) => before(press.at, cols).unwrap_or(press.at),
        (false, false) => after(press.at, cols, end).unwrap_or(press.at),
    };
    let finish = match (take_under, backwards) {
        (true, _) => at,
        (false, true) => after(at, cols, end).unwrap_or(at),
        (false, false) => before(at, cols).unwrap_or(at),
    };
    let none = (!take_pressed && (same || finish == press.at)) || (!take_under && start == at);
    (!none).then_some((start, finish))
}

/// The cell before `at`, at the end of the row above for the first of a row.
fn before(at: TerminalPoint, cols: u16) -> Option<TerminalPoint> {
    if at.col > 0 {
        return Some(TerminalPoint { col: at.col - 1, ..at });
    }
    let row = at.row.get().checked_sub(1)?;
    Some(TerminalPoint { row: RowNumber::new(row), col: cols.saturating_sub(1) })
}

/// The cell after `at`, at the start of the row below for the last of a row, unless that's past the screen's end.
fn after(at: TerminalPoint, cols: u16, end: RowNumber) -> Option<TerminalPoint> {
    if at.col + 1 < cols {
        return Some(TerminalPoint { col: at.col + 1, ..at });
    }
    let row = at.row.plus(1);
    (row < end).then_some(TerminalPoint { row, col: 0 })
}
