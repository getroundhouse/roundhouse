//! A terminal's cells by number, for what a client selects: the ends of a word or a line, by Ghostty's rules, and the
//! text between two cells, by Ghostty's formatter, as `protocol::terminal::TerminalTextReply` says. Neither sets the
//! terminal's selection or moves what it shows, since the worker's copy is every client's.

use libghostty_vt::Terminal;
use libghostty_vt::fmt::{Format, Formatter};
use libghostty_vt::point::{Coordinate, Point, Space};
use libghostty_vt::selection::{SelectLineOptions, SelectWordOptions, Selection};
use protocol::terminal::{
    Bounds, MOST_TEXT_BYTES, Numbering, RowNumber, TerminalBoundsReply, TerminalPoint, TerminalRange, TerminalTextReply,
};

use super::counting::Numbered;
use crate::{Mirror, MirrorError};

impl Mirror {
    /// The ends of the word or the line at `at`, in `numbering`, as a double or a triple click selects them.
    pub fn bounds(
        &mut self,
        numbering: Numbering,
        at: TerminalPoint,
        of: Bounds,
    ) -> Result<TerminalBoundsReply, MirrorError> {
        let now = self.numbered()?;
        let mut reply = TerminalBoundsReply { numbering: now.numbering, range: None };
        let Some(at) = self.kept(now, numbering, at)? else { return Ok(reply) };
        let at = self.terminal.grid_ref(at)?;
        let selection = match of {
            Bounds::Word => self.terminal.select_word(SelectWordOptions::new(at))?,
            Bounds::Line => self.terminal.select_line(SelectLineOptions::new(at))?,
            Bounds::Unknown => None,
        };
        if let Some(selection) = selection {
            let from = numbered(&self.terminal, now, &selection.start())?;
            let to = numbered(&self.terminal, now, &selection.end())?;
            reply.range = from.zip(to).map(|(from, to)| TerminalRange { from, to });
        }
        Ok(reply)
    }

    /// The text of the cells in `range`, in `numbering`, or of the columns between its two cells in each of its rows
    /// for a `rectangle`, a page at a time.
    pub fn text(
        &mut self,
        numbering: Numbering,
        range: TerminalRange,
        rectangle: bool,
    ) -> Result<TerminalTextReply, MirrorError> {
        self.paged(numbering, range, rectangle, MOST_TEXT_BYTES)
    }

    /// The text of the cells in `range`, as [`Mirror::text`] says, a page of at most `most_bytes` at a time.
    pub(in crate::mirror) fn paged(
        &mut self,
        numbering: Numbering,
        range: TerminalRange,
        rectangle: bool,
        most_bytes: usize,
    ) -> Result<TerminalTextReply, MirrorError> {
        let now = self.numbered()?;
        let mut reply = TerminalTextReply { numbering: now.numbering, text: String::new(), rest: None, joins: false };
        let (cols, rows) = (self.terminal.cols()?, self.terminal.rows()?);
        let last = TerminalPoint { row: now.top.plus(u64::from(rows) - 1), col: cols - 1 };
        let from = range.from.max(TerminalPoint { row: now.first, col: 0 });
        let to = range.to.min(last);
        if numbering != now.numbering || from > to {
            return Ok(reply);
        }
        // Text is about a byte a cell, so a page is first tried at as many rows as come to a page at that, and then
        // at fewer until its text fits.
        let mut most_rows = u64::try_from(most_bytes / usize::from(cols)).unwrap_or(u64::MAX).max(1);
        loop {
            let (page_to, cut) = self.page_end(now, from, to, most_rows, rectangle)?;
            let text = match cut {
                // A line's end for each but the last, whose end is the page's.
                Cut::Blank => "\n".repeat(usize::try_from(page_to.row.get() - from.row.get()).unwrap_or(0)),
                Cut::LineEnd | Cut::MidLine | Cut::Last => {
                    self.format(now, from, page_to, rectangle, cut != Cut::MidLine)?
                }
            };
            if text.len() <= most_bytes || most_rows == 1 {
                reply.rest = (cut != Cut::Last)
                    .then(|| TerminalPoint { row: page_to.row.plus(1), col: if rectangle { from.col } else { 0 } });
                (reply.text, reply.joins) = (text, cut == Cut::MidLine);
                return Ok(reply);
            }
            most_rows /= 2;
        }
    }

    /// Where a page from `from` to at most `to` ends, at most `most_rows` rows on, and how. It ends where a line with
    /// something on it does, so the pages joined are the same text as one would be, since Ghostty leaves out the blank
    /// lines at the end of what it copies, and the spaces at the end of a line, and a page that ended on either would
    /// lose them. Only a line longer than a page is cut in the middle, and a page of blank rows is those rows alone.
    fn page_end(
        &self,
        now: Numbered,
        from: TerminalPoint,
        to: TerminalPoint,
        most_rows: u64,
        rectangle: bool,
    ) -> Result<(TerminalPoint, Cut), MirrorError> {
        let last = from.row.plus(most_rows - 1);
        if last >= to.row {
            return Ok((to, Cut::Last));
        }
        let end = |row| TerminalPoint { row, col: if rectangle { to.col } else { u16::MAX } };
        let (mut row, mut wrapped) = (last, false);
        loop {
            let wraps = !rectangle && self.wraps(now, row)?;
            if !wraps && !self.blank(now, row, from, to, rectangle)? {
                // Blank to the end is left out, as Ghostty leaves it out, so this is the last page.
                return Ok(if self.blank_after(now, row, from, to, rectangle)? {
                    (to, Cut::Last)
                } else {
                    (end(row), Cut::LineEnd)
                });
            }
            wrapped |= wraps;
            if row == from.row {
                break;
            }
            row = RowNumber::new(row.get() - 1);
        }
        if wrapped {
            return Ok((end(last), Cut::MidLine));
        }
        Ok(if self.blank_after(now, last, from, to, rectangle)? { (to, Cut::Last) } else { (end(last), Cut::Blank) })
    }

    /// Whether what's in the range on every row after `row`, up to `to`'s, is blank.
    fn blank_after(
        &self,
        now: Numbered,
        row: RowNumber,
        from: TerminalPoint,
        to: TerminalPoint,
        rectangle: bool,
    ) -> Result<bool, MirrorError> {
        let mut row = row.plus(1);
        while row <= to.row {
            if !self.blank(now, row, from, to, rectangle)? {
                return Ok(false);
            }
            row = row.plus(1);
        }
        Ok(true)
    }

    /// Whether the cells of `row` in the range from `from` to `to` have nothing in them but spaces.
    fn blank(
        &self,
        now: Numbered,
        row: RowNumber,
        from: TerminalPoint,
        to: TerminalPoint,
        rectangle: bool,
    ) -> Result<bool, MirrorError> {
        let start = if rectangle || row == from.row { from.col } else { 0 };
        let end = if rectangle || row == to.row { to.col } else { u16::MAX };
        Ok(self
            .format(now, TerminalPoint { row, col: start }, TerminalPoint { row, col: end }, false, true)?
            .is_empty())
    }

    fn wraps(&self, now: Numbered, row: RowNumber) -> Result<bool, MirrorError> {
        let at = place(now, TerminalPoint { row, col: 0 });
        Ok(self.terminal.grid_ref(at)?.row()?.is_wrapped()?)
    }

    /// The text from `from` to `to`, both in what the terminal keeps, as Ghostty copies it: each row of a rectangle a
    /// line of its own, and otherwise the rows a line wrapped joined again. The spaces at the end of each line are left
    /// out if `trims`, which a page that ends in the middle of a line doesn't, since they aren't the line's end.
    fn format(
        &self,
        now: Numbered,
        from: TerminalPoint,
        to: TerminalPoint,
        rectangle: bool,
        trims: bool,
    ) -> Result<String, MirrorError> {
        let cols = self.terminal.cols()?;
        let to = TerminalPoint { col: to.col.min(cols - 1), ..to };
        let (start, end) = (self.terminal.grid_ref(place(now, from))?, self.terminal.grid_ref(place(now, to))?);
        let selection = Selection::new(start, end, rectangle);
        let mut formatter = Formatter::new(&self.terminal)?;
        formatter.set_format(Format::Plain)?.set_unwrap(!rectangle)?.set_trim(trims)?;
        // SAFETY: Both ends are this terminal's, from its screen showing, and nothing changes it before they're read,
        // since this holds it borrowed throughout.
        unsafe { formatter.set_selection(Some(&selection))? };
        self.history_read();
        Ok(String::from_utf8_lossy(&formatter.format_alloc(None)?).into_owned())
    }

    /// Where `at` is in the terminal, if it's in `numbering`, the one it's in now, and among the rows it keeps.
    pub(super) fn kept(
        &self,
        now: Numbered,
        numbering: Numbering,
        at: TerminalPoint,
    ) -> Result<Option<Point>, MirrorError> {
        let last = now.top.plus(u64::from(self.terminal.rows()?));
        let kept =
            numbering == now.numbering && at.row >= now.first && at.row < last && at.col < self.terminal.cols()?;
        Ok(kept.then(|| place(now, at)))
    }
}

/// Where a cell the terminal keeps is, counted from the oldest row it keeps, as its rows are numbered from.
fn place(now: Numbered, at: TerminalPoint) -> Point {
    let y = u32::try_from(at.row.get() - now.first.get()).unwrap_or(u32::MAX);
    Point::Screen(Coordinate { x: at.col, y })
}

/// The cell a reference is to, by number.
pub(super) fn numbered(
    terminal: &Terminal<'_, '_>,
    now: Numbered,
    at: &libghostty_vt::grid_ref::GridRef<'_>,
) -> Result<Option<TerminalPoint>, MirrorError> {
    let point = terminal.point_from_grid_ref(at, Space::Screen)?;
    Ok(point.map(|point| TerminalPoint { row: now.first.plus(u64::from(point.y)), col: point.x }))
}

/// How a page ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cut {
    /// At the end of what was asked for.
    Last,
    /// Where a line with something on it ends.
    LineEnd,
    /// In the middle of a line longer than a page.
    MidLine,
    /// After rows with nothing on them, with something after.
    Blank,
}
