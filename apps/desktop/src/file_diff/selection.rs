//! Selecting the diff's text with the pointer, and copying it. What's selected is held here, as a row and how far into
//! its text, rather than by the text the window draws, so it stays selected as its rows scroll out of sight, and a copy
//! takes each line from the file, tabs and all. Side by side, a selection keeps to the side it started on, as a column
//! of code is what's copied from there.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::input::{Copy, SelectAll};
use gpui_kit::{ClipboardItem, Context, MouseDownEvent, Pixels, Point, TextLayout, Window};

use super::FileDiff;
use super::layout::{self, Row};
use crate::patch::LineKind;

/// The key context the diff's keys for copying are bound in.
pub(crate) const KEY_CONTEXT: &str = "FileDiff";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Selection {
    /// Where it started.
    anchor: Spot,
    /// Where it's got to.
    head: Spot,
    /// Side by side, the side it started on, as whether it's the old one.
    side: Option<bool>,
}

/// A place in the diff's text: a row, and how far into its text in bytes, as drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Spot {
    row: usize,
    at: usize,
}

/// A line's text as it was drawn in the last frame, for finding where the pointer is in it.
pub(super) struct Drawn {
    pub(super) row: usize,
    /// Whether it's the old side's text.
    pub(super) old: bool,
    pub(super) layout: TextLayout,
}

/// The lines drawn in the last frame.
pub(super) type DrawnLines = Rc<RefCell<Vec<Drawn>>>;

impl Selection {
    /// What's selected of the text of `row`, on the old side or the new, which is `length` bytes long. A row the
    /// selection runs through is selected whole, even when it's empty, so it's copied as a line.
    pub(super) fn within(&self, row: usize, old: bool, length: usize) -> Option<Range<usize>> {
        let (start, end) = self.ends();
        if self.side.is_some_and(|side| side != old) || row < start.row || row > end.row {
            return None;
        }
        let from = if row == start.row { start.at.min(length) } else { 0 };
        let to = if row == end.row { end.at.min(length) } else { length };
        Some(from..to.max(from))
    }

    fn ends(&self) -> (Spot, Spot) {
        (self.anchor.min(self.head), self.anchor.max(self.head))
    }

    fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

/// The side and number of the line a row draws, if it draws one on `side`, which side by side is the side a selection
/// keeps to. A line both sides have is drawn, and copied, as the new side has it.
fn text_of(row: &Row, side: Option<bool>) -> Option<(bool, u32)> {
    match (row, side) {
        (Row::Line { kind: LineKind::Removed, old, .. }, _) => old.map(|old| (true, old)),
        (Row::Line { new, .. }, _) => new.map(|new| (false, new)),
        (Row::Pair { old, .. }, Some(true)) => old.map(|old| (true, old)),
        (Row::Pair { new, .. }, _) => new.map(|new| (false, new)),
        (Row::Fold(_) | Row::Comment(_) | Row::Draft, _) => None,
    }
}

impl FileDiff {
    /// Starts selecting where the button went down, or with Shift, takes the selection there. Only over a line's
    /// text, so a press in a comment or on a fold's arrow is left to what's there.
    pub(super) fn select_from(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let side = self.selection.and_then(|selection| selection.side).filter(|_| event.modifiers.shift);
        let Some((spot, old)) = self.spot_at(event.position, side, false) else { return };
        let split = self.rows.iter().any(|row| matches!(row, Row::Pair { .. }));
        self.selection = Some(match self.selection {
            Some(selection) if event.modifiers.shift => Selection { head: spot, ..selection },
            _ => Selection { anchor: spot, head: spot, side: split.then_some(old) },
        });
        self.selecting = true;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Takes the selection to where the pointer is, while the button that started it is down. Past the top or the
    /// bottom of the list, it scrolls the list that way, by how far past it is.
    pub(super) fn select_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(selection) = self.selection.filter(|_| self.selecting) else { return };
        let viewport = self.list.viewport();
        let past = (position.y - viewport.bottom()).max(Pixels::ZERO) + (position.y - viewport.top()).min(Pixels::ZERO);
        if past != Pixels::ZERO {
            self.list.scroll_by(past);
        }
        if let Some((head, _)) = self.spot_at(position, selection.side, true) {
            self.selection = Some(Selection { head, ..selection });
        }
        cx.notify();
    }

    pub(super) fn stop_selecting(&mut self) {
        self.selecting = false;
    }

    /// The place in the text drawn under `position`, on `side` if it's given, and the side it's on. Over a row that
    /// has no text, such as a comment's, it's `None`, or the start of the next line drawn when `nearest`, and past the
    /// last line, the end of it.
    fn spot_at(&self, position: Point<Pixels>, side: Option<bool>, nearest: bool) -> Option<(Spot, bool)> {
        let drawn = self.drawn.borrow();
        let middle = self.list.viewport().center().x;
        let side =
            side.or_else(|| self.rows.iter().any(|row| matches!(row, Row::Pair { .. })).then(|| position.x < middle));
        let lines = || drawn.iter().filter(|line| side.is_none_or(|side| line.old == side));
        let spot = |line: &Drawn| {
            let at = line.layout.index_for_position(position).unwrap_or_else(|closest| closest);
            (Spot { row: line.row, at }, line.old)
        };
        let row_band = |line: &&Drawn| {
            let bounds = line.layout.bounds();
            bounds.top() <= position.y && position.y < bounds.top() + line.layout.line_height()
        };
        if let Some(line) = lines().find(row_band) {
            return Some(spot(line));
        }
        if !nearest {
            return None;
        }
        let below = lines().filter(|line| line.layout.bounds().top() > position.y).min_by_key(|line| line.row);
        match below {
            Some(line) => Some((Spot { row: line.row, at: 0 }, line.old)),
            None => lines().max_by_key(|line| line.row).map(|line| (Spot { row: line.row, at: usize::MAX }, line.old)),
        }
    }

    /// Keeps the selection on the same lines once the rows are laid out again, or drops it when one of its ends is gone.
    pub(super) fn keep_selection(&mut self, was: &[Row], now: &[Row]) {
        let Some(selection) = self.selection else { return };
        let moved = |spot: Spot| {
            let row = layout::same_row(was, now, spot.row).filter(|&row| now.get(row) == was.get(spot.row))?;
            Some(Spot { row, ..spot })
        };
        self.selection = moved(selection.anchor).zip(moved(selection.head)).map(|(anchor, head)| Selection {
            anchor,
            head,
            ..selection
        });
    }

    pub(super) fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// Selects every line, or side by side, every line of the new side.
    pub(super) fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let split = self.rows.iter().any(|row| matches!(row, Row::Pair { .. }));
        let (anchor, head) = (Spot { row: 0, at: 0 }, Spot { row: self.rows.len().saturating_sub(1), at: usize::MAX });
        self.selection = Some(Selection { anchor, head, side: split.then_some(false) });
        cx.notify();
    }

    /// What's selected, a line apart, as the file has it.
    pub fn selected_text(&self) -> Option<String> {
        let (selection, shown) = (self.selection.filter(|selection| !selection.is_empty())?, self.shown.as_ref()?);
        let (start, end) = selection.ends();
        let rows = self.rows.get(start.row..=end.row.min(self.rows.len().saturating_sub(1)))?;
        let lines: Vec<String> = (start.row..)
            .zip(rows)
            .filter_map(|(ix, row)| {
                let (old, number) = text_of(row, selection.side)?;
                let side = if old { &shown.sides.old } else { &shown.sides.new };
                let range = selection.within(ix, old, side.line(number)?.text.len())?;
                side.as_in_the_file(number, range)
            })
            .collect();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }
}
