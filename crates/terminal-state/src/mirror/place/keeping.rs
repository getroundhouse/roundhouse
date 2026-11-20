//! Finding a person's place again in a new copy, by the text of its rows.

use libghostty_vt::Terminal;
use libghostty_vt::fmt::{Format, Formatter};
use libghostty_vt::point::{Coordinate, Point, Space};
use libghostty_vt::selection::Selection;
use libghostty_vt::terminal::ScrollViewport;

use crate::{Mirror, MirrorError};

impl Mirror {
    /// Scrolls to where `old` is scrolled to, and selects what's selected there, if this copy has those rows, so a new
    /// copy of the same terminal, such as one a client starts again from a new screen, keeps the person's place. The
    /// rows are found by their text, since two copies needn't number them alike: output one missed adds rows at the
    /// bottom of the other, and each drops its oldest from the top as its history fills. They're the rows selected,
    /// and the rows in view while the history is scrolled back, and they're taken to be the same rows only where this
    /// copy has them once, so the place is never another one with the same text.
    ///
    /// Otherwise this copy stays as a new one is, at the bottom with nothing selected: when the rows have gone from its
    /// history, or were cleared, or repeat, as a program that writes the same line over and over has them, or when the
    /// copies are of different sizes or on different screens, which lay rows out differently. A drag under way in `old`
    /// doesn't go on in this one, though what it selected is kept.
    pub fn keep_place_of(&mut self, old: &Mirror) -> Result<(), MirrorError> {
        // With the view at the bottom and nothing selected, as it usually is, there's nothing to read.
        if old.terminal.viewport_active()? && old.terminal.selection()?.is_none() {
            return Ok(());
        }
        if self.size()? != old.size()? || self.terminal.active_screen()? != old.terminal.active_screen()? {
            return Ok(());
        }
        let Some(place) = Place::in_copy(&old.terminal)? else { return Ok(()) };
        let Some(moved) = place.moved_in(&rows(&self.terminal)?) else { return Ok(()) };
        place.take_to(&mut self.terminal, moved)
    }
}

/// Where a person is in a copy, in rows of the whole of its screen, the history included, from its top.
struct Place {
    /// The row the view starts at, while the history is scrolled back.
    scrolled_to: Option<u32>,
    /// The ends of what's selected, in the order they were made, and whether it's a rectangle.
    selected: Option<(Coordinate, Coordinate, bool)>,
    /// The row the place is found again from, and the text of that row and those after it.
    found_by: (u32, Vec<String>),
}

impl Place {
    /// Where the person is in `terminal`, unless it's only the bottom.
    fn in_copy(terminal: &Terminal<'_, '_>) -> Result<Option<Place>, MirrorError> {
        let scrolled_to = if terminal.viewport_active()? { None } else { Some(row(terminal.scrollbar()?.offset)) };
        let selected = match terminal.selection()? {
            Some(selection) => {
                let start = terminal.point_from_grid_ref(&selection.start(), Space::Screen)?;
                let end = terminal.point_from_grid_ref(&selection.end(), Space::Screen)?;
                start.zip(end).map(|(start, end)| (start, end, selection.is_rectangle()))
            }
            None => None,
        };
        // The rows in view only while the history is scrolled back, since at the bottom they change with any output
        // missed.
        let shown = u32::from(terminal.rows()?);
        let in_view = scrolled_to.map(|top| (top, top + shown - 1));
        let in_selection = selected.map(|(start, end, _)| (start.y.min(end.y), start.y.max(end.y)));
        let Some((first, last)) = in_view.into_iter().chain(in_selection).reduce(|(a, b), (c, d)| (a.min(c), b.max(d)))
        else {
            return Ok(None);
        };
        let rows = rows(terminal)?;
        let found_by = (first..=last).map(|y| rows.get(index(y)).cloned().unwrap_or_default()).collect();
        Ok(Some(Place { scrolled_to, selected, found_by: (first, found_by) }))
    }

    /// How many rows down the place has moved in a copy whose rows are `rows`, up for fewer than none. None unless its
    /// rows are there exactly once. They can be either way from where they were: output missed moves them up, and a
    /// copy that has dropped fewer of its oldest rows than the other has them lower down.
    fn moved_in(&self, rows: &[String]) -> Option<i64> {
        let (first, found_by) = &self.found_by;
        let mut found = rows.windows(found_by.len()).enumerate().filter(|(_, those)| *those == found_by.as_slice());
        let (start, _) = found.next()?;
        if found.next().is_some() {
            return None;
        }
        Some(signed(start) - signed(index(*first)))
    }

    /// Scrolls `terminal` to the place, `moved` rows down from where it was, and selects what was selected there.
    fn take_to(&self, terminal: &mut Terminal<'_, '_>, moved: i64) -> Result<(), MirrorError> {
        let moving = |y: u32| u32::try_from(i64::from(y) + moved).ok();
        if let Some(top) = self.scrolled_to.and_then(moving) {
            terminal.scroll_viewport(ScrollViewport::Row(index(top)));
        }
        let Some((start, end, rectangle)) = self.selected else { return Ok(()) };
        let (Some(start_y), Some(end_y)) = (moving(start.y), moving(end.y)) else { return Ok(()) };
        let start = terminal.grid_ref(Point::Screen(Coordinate { y: start_y, ..start }))?;
        let end = terminal.grid_ref(Point::Screen(Coordinate { y: end_y, ..end }))?;
        terminal.set_selection(Some(&Selection::new(start, end, rectangle)))?;
        Ok(())
    }
}

/// The text of every row of the screen showing, the history included, from its top, one for each row, as Ghostty lays
/// them out, with a line that wrapped left as the rows it wrapped onto.
fn rows(terminal: &Terminal<'_, '_>) -> Result<Vec<String>, MirrorError> {
    let mut formatter = Formatter::new(terminal)?;
    formatter.set_format(Format::Plain)?.set_unwrap(false)?.set_trim(true)?;
    let text = formatter.format_alloc(None)?;
    let mut rows: Vec<String> = String::from_utf8_lossy(&text).split('\n').map(str::to_owned).collect();
    // The blank rows at the bottom, which the formatter leaves out.
    rows.resize(terminal.total_rows()?, String::new());
    Ok(rows)
}

/// A row of the whole screen, which is far fewer than `u32::MAX`, as Ghostty's own points take it.
fn row(at: u64) -> u32 {
    u32::try_from(at).unwrap_or(u32::MAX)
}

fn index(row: u32) -> usize {
    usize::try_from(row).unwrap_or(usize::MAX)
}

fn signed(index: usize) -> i64 {
    i64::try_from(index).unwrap_or(i64::MAX)
}
