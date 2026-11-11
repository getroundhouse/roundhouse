//! Where a transcript is scrolled to, and what's known of its rows' heights, between one frame and the next.

use std::ops::Range;

use gpui_kit::{Bounds, Pixels, px};
use look::motion::Scrolling;

use super::heights::Heights;
use super::layout::{Anchor, Start};

mod heading;

/// A place in a transcript: a row, and how far into it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Place {
    pub row: usize,
    /// How far below the row's top.
    pub offset: Pixels,
}

#[derive(Default)]
pub(super) struct State {
    pub(super) heights: Heights,
    /// The place at the top of what's shown, as the last frame laid it out or as something has since sent it to.
    top: Place,
    /// How far down the session the top has moved from that place since, by the wheel. The wheel moves it without
    /// knowing how tall the rows it passes are, and the next frame finds which row it comes to with the heights it
    /// measures as it lays them out.
    moved: Pixels,
    /// The row the next frame keeps where it is in the window, as [`Anchor`] says. `None` once something has sent
    /// the list to a place, until a frame lays it out there.
    anchor: Option<Anchor>,
    /// What it keeps to between frames.
    keeping: Keeping,
    /// Where the rows are drawn, as of the last frame.
    pub(super) viewport: Bounds<Pixels>,
    /// The rows any of which showed in the last frame.
    pub(super) shown: Range<usize>,
    /// The runs of rows that are loaded, in order, which are the ones there's anything to measure of.
    loaded: Vec<Range<usize>>,
    /// Which measuring each row's height is from, by [`State::measuring`], or 0 for one whose height has gone out of
    /// date by itself. A row whose height isn't from this one keeps it as a guess, and is measured again when it's
    /// drawn, or a few at a time between.
    measured_at: Vec<u32>,
    /// Counts the times every row's height has gone out of date at once, from 1: each width the list is drawn at, and
    /// each change that can alter any row.
    measuring: u32,
    /// The count as of the last change that can alter any row. A height from this count or later is out of date only
    /// by the list's width.
    altered_at: u32,
    /// Whether the last frame was drawn at another width than the one before it.
    width_is_new: bool,
    /// Every row's height as it was known or guessed when a drag of the scrollbar's thumb began. Until the drag ends,
    /// the scrollbar goes by these alone, so a place on its track is the same row however long the pointer rests
    /// there. The heights go on being put right meanwhile, as rows load under the thumb and are measured, and going
    /// by those would move the page under a pointer that's holding still.
    dragging: Option<Heights>,
    /// How many times a row has been drawn, for a test to see that few are.
    #[cfg(feature = "testing")]
    pub(super) rows_drawn: u64,
}

impl State {
    /// A session with nothing in it yet, which keeps to its latest row.
    pub(super) fn new() -> Self {
        Self { keeping: Keeping::Latest, measuring: 1, altered_at: 1, ..Self::default() }
    }

    /// Makes it `rows` rows, of which the runs in `loaded` are loaded, in order. Rows only come and go at the end.
    ///
    /// A page of history that loads, or one that's let go, puts every row's height out of date. What a row shows
    /// can depend on another being loaded: an event of a call is drawn in its call's row while that's loaded, and
    /// where it is once it isn't. Rows that come and go at the end alter none before them.
    pub(super) fn set_rows(&mut self, rows: usize, loaded: Vec<Range<usize>>) {
        self.heights.set_rows(rows);
        self.measured_at.resize(rows, 0);
        self.anchor = self.anchor.filter(|anchor| anchor.row < rows);
        let loaded = joined(loaded);
        if !same_pages(&self.loaded, &loaded) {
            self.all_out_of_date();
        }
        self.loaded = loaded;
    }

    pub(super) fn tall(&self) -> Pixels {
        self.viewport.size.height
    }

    /// How far the session can be scrolled.
    pub(super) fn furthest(&self) -> Pixels {
        (self.heights.total() - self.tall()).max(px(0.))
    }

    /// The row at the top of what's shown, as of the last frame.
    pub(super) fn top(&self) -> Place {
        self.top
    }

    /// Whether someone has scrolled away from the latest row.
    pub(super) fn is_scrolled_up(&self) -> bool {
        self.furthest() > px(0.) && !self.is_following()
    }

    /// Whether it's keeping to the latest row.
    fn is_following(&self) -> bool {
        matches!(self.keeping, Keeping::Latest)
    }

    /// Goes to the latest row, and keeps to it as more arrives.
    pub(super) fn go_to_latest(&mut self) {
        self.keeping = Keeping::Latest;
    }

    /// How far down the session the top of what's shown is.
    pub(super) fn scrolled(&self) -> Pixels {
        if self.is_following() {
            return self.furthest();
        }
        self.down(&self.heights).clamp(px(0.), self.furthest())
    }

    /// How far down the session the top of what's shown is going by `heights`, wherever the session ends.
    fn down(&self, heights: &Heights) -> Pixels {
        heights.above(self.top.row) + self.top.offset + self.moved
    }

    /// The wheel turned by `turned`, which is positive to bring earlier rows down into view. On its way down to the
    /// latest row, it stops there and goes by the wheel.
    pub(super) fn scroll_by(&mut self, turned: Pixels) {
        match self.keeping {
            // There's nothing further down.
            Keeping::Latest if turned <= px(0.) => return,
            // From where keeping to the latest row had it.
            Keeping::Latest => self.go_to(self.furthest()),
            Keeping::Place | Keeping::ToLatest(_) => {}
        }
        self.keeping = Keeping::Place;
        self.moved -= turned;
    }

    /// Goes to a place `down` the session, by what its rows are known and guessed to come to.
    fn go_to(&mut self, down: Pixels) {
        let (row, offset) = self.heights.row_at(down.clamp(px(0.), self.furthest()));
        self.send_to(Place { row, offset });
    }

    pub(super) fn go_to_row(&mut self, row: usize) {
        if row < self.heights.rows() {
            self.send_to(Place { row, offset: px(0.) });
            self.keeping = Keeping::Place;
        }
    }

    /// Puts `top` at the window's top, from which the next frame lays the rows out and chooses which to keep where
    /// it is.
    fn send_to(&mut self, top: Place) {
        (self.top, self.moved, self.anchor) = (top, px(0.), None);
    }

    /// Someone opened or closed something in `row`, which keeps it where it is in the window, as [`Anchor`] says,
    /// if it's in sight.
    pub(super) fn keep_still(&mut self, row: usize) {
        let top = self.heights.above(row) - self.heights.above(self.top.row) - self.top.offset;
        if row >= self.top.row && top < self.tall() {
            self.anchor = Some(Anchor { row, top });
        }
    }

    /// Where to lay the rows out from. A top that's moved more than `walked` is found from what the rows are guessed
    /// to come to instead, since finding it row by row would measure every row between.
    pub(super) fn lay_from(&self, walked: Pixels) -> Start {
        if self.is_following() {
            return Start::Latest;
        }
        if self.moved.abs() <= walked {
            return match self.anchor {
                Some(at) => Start::Anchored { at, moved: self.moved },
                None => Start::Top { at: self.top, moved: self.moved },
            };
        }
        let (row, offset) = self.heights.row_at(self.down(&self.heights).max(px(0.)));
        Start::Top { at: Place { row, offset }, moved: px(0.) }
    }

    /// The rows were laid out with `top` at the top of the window, `anchor` to keep where it is in the next frame,
    /// and with the latest row's end in sight or not. Someone who has scrolled back down to that is keeping to it
    /// again, and so is a list on its way down to it.
    pub(super) fn laid_out(&mut self, top: Place, anchor: Option<Anchor>, end_in_sight: bool) {
        (self.top, self.moved, self.anchor) = (top, px(0.), anchor);
        if end_in_sight {
            self.go_to_latest();
        }
    }

    /// The list is drawn in `viewport`, which puts every row's height out of date if it's another width than last
    /// time.
    pub(super) fn drawn_at(&mut self, viewport: Bounds<Pixels>) {
        self.width_is_new = self.viewport.size.width != viewport.size.width;
        if self.width_is_new {
            self.measuring += 1;
        }
        self.viewport = viewport;
    }

    /// Notes what a row was measured at, as the list is now.
    pub(super) fn measured(&mut self, row: usize, height: Pixels) {
        self.heights.measure(row, height);
        self.up_to_date(row);
    }

    /// There's nothing to measure of `row` until it changes, such as one that's gone since the list was told what's
    /// loaded. It's marked to measure again when it's back.
    pub(super) fn up_to_date(&mut self, row: usize) {
        if let Some(at) = self.measured_at.get_mut(row) {
            *at = self.measuring;
        }
    }

    /// These rows have changed, so each is to be measured again.
    pub(super) fn out_of_date(&mut self, rows: Range<usize>) {
        let rows = rows.start.min(self.measured_at.len())..rows.end.min(self.measured_at.len());
        self.measured_at[rows].fill(0);
    }

    /// Every row may have changed height.
    pub(super) fn all_out_of_date(&mut self) {
        self.measuring += 1;
        self.altered_at = self.measuring;
    }

    fn is_loaded(&self, row: usize) -> bool {
        let run = self.loaded.partition_point(|run| run.end <= row);
        self.loaded.get(run).is_some_and(|run| run.contains(&row))
    }

    fn is_out_of_date(&self, row: usize) -> bool {
        self.measured_at.get(row).is_some_and(|at| *at != self.measuring)
    }

    /// How tall a loaded row is, if it's been measured as the list is now and hasn't changed since. Any other row
    /// is drawn to find out, or to find it isn't there to draw.
    ///
    /// A row that showed nothing at another width shows nothing at this one, so that's known too. Most of a
    /// session's rows show nothing, and a run of them in the window would otherwise be drawn on every frame of a
    /// sidebar's slide.
    pub(super) fn kept(&self, row: usize) -> Option<Pixels> {
        let measured_at = *self.measured_at.get(row)?;
        if !self.is_loaded(row) {
            return None;
        }
        let height = self.heights.measured(row)?;
        let only_the_width_since = measured_at >= self.altered_at && height == px(0.);
        (measured_at == self.measuring || only_the_width_since).then_some(height)
    }

    /// How many rows are loaded, as the list was last told.
    #[cfg(feature = "testing")]
    pub(super) fn loaded_rows(&self) -> usize {
        self.loaded.iter().map(ExactSizeIterator::len).sum()
    }

    /// The loaded rows whose heights are out of date, nearest the top of what's shown first, and at most `most` of
    /// them. Measured a few to a frame, they put the guesses right without holding a frame up, so the scrollbar
    /// settles on the session's real height a moment after a page loads or the width changes.
    pub(super) fn out_of_date_near_the_top(&self, most: usize) -> Vec<usize> {
        let far = |row: &usize| row.abs_diff(self.top.row);
        let mut rows: Vec<usize> =
            self.loaded.iter().flat_map(Clone::clone).filter(|row| self.is_out_of_date(*row)).collect();
        if rows.len() > most && most > 0 {
            // Only the nearest are put in order, since a new width leaves every loaded row here.
            rows.select_nth_unstable_by_key(most - 1, far);
        }
        rows.truncate(most);
        rows.sort_by_key(far);
        rows
    }

    /// The rows to measure out of sight in this frame, as [`State::out_of_date_near_the_top`] gives them, and
    /// whether there are more for the frames after.
    ///
    /// None are measured in a frame drawn at another width than the last, only the frame after is asked for. A
    /// sidebar sliding or a window being resized changes the width on every frame, and what was measured in one
    /// would be out of date in the next.
    pub(super) fn to_measure_out_of_sight(&self, most: usize) -> (Vec<usize>, bool) {
        let most = if self.width_is_new { 0 } else { most };
        let mut rows = self.out_of_date_near_the_top(most + 1);
        let more = rows.len() > most;
        rows.truncate(most);
        (rows, more)
    }

    /// A drag of the scrollbar's thumb begins, which stops the list on its way down to the latest row.
    pub(super) fn start_drag(&mut self) {
        self.dragging = Some(self.heights.clone());
        if let Keeping::ToLatest(_) = self.keeping {
            self.keeping = Keeping::Place;
        }
    }

    pub(super) fn end_drag(&mut self) {
        self.dragging = None;
    }

    /// The heights the scrollbar goes by.
    fn for_the_bar(&self) -> &Heights {
        self.dragging.as_ref().unwrap_or(&self.heights)
    }

    /// How tall the scrollbar takes the session to be.
    pub(super) fn bar_tall(&self) -> Pixels {
        self.for_the_bar().total().max(self.tall())
    }

    /// How far down the scrollbar takes the session to be scrolled.
    pub(super) fn bar_scrolled(&self) -> Pixels {
        match &self.dragging {
            // Where the row at the top was when the drag began, so the thumb stays under the pointer.
            Some(heights) => {
                let furthest = (heights.total() - self.tall()).max(px(0.));
                self.down(heights).clamp(px(0.), furthest)
            }
            None => self.scrolled(),
        }
    }

    /// The scrollbar scrolls to `down`. A drag of the thumb goes to the row that was that far down when the drag
    /// began. Anything else moves by the difference from where it is, as the wheel does, so it passes the rows
    /// between at the heights they turn out to have. Either way, as far down as the scrollbar goes is the latest
    /// row, which is kept to again.
    pub(super) fn bar_scroll_to(&mut self, down: Pixels) {
        let furthest = if let Some(heights) = &self.dragging {
            let furthest = (heights.total() - self.tall()).max(px(0.));
            let (row, offset) = heights.row_at(down.clamp(px(0.), furthest));
            self.send_to(Place { row, offset });
            self.keeping = Keeping::Place;
            furthest
        } else {
            let turned = self.scrolled() - down;
            if turned.abs() > NO_MOVE {
                self.scroll_by(turned);
            }
            self.furthest()
        };
        if furthest > px(0.) && down >= furthest {
            self.go_to_latest();
        }
    }
}

/// What the list keeps to between frames.
#[derive(Debug, Default)]
enum Keeping {
    /// The latest row, which it does until someone scrolls away from it, and again once they're back.
    #[default]
    Latest,
    /// The place at the top, wherever someone scrolled the list or sent it.
    Place,
    /// The latest row, which it's on its way down to by itself, as the button back to it sends it.
    ToLatest(Scrolling),
}

/// The runs in order with each that starts where the one before it ends made one with it, such as the streaming
/// tail's row after the rows recorded, so a row added at the end only lengthens the last run.
fn joined(runs: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut joined: Vec<Range<usize>> = Vec::with_capacity(runs.len());
    for run in runs {
        match joined.last_mut() {
            Some(last) if last.end == run.start => last.end = run.end,
            Some(_) | None => joined.push(run),
        }
    }
    joined
}

/// Whether two lists of joined runs of loaded rows hold the same pages of history, so they differ only in where the last run ends,
/// which is where rows come and go as an agent works.
fn same_pages(was: &[Range<usize>], now: &[Range<usize>]) -> bool {
    let starts = |runs: &[Range<usize>]| runs.iter().map(|run| run.start).collect::<Vec<_>>();
    let ends_before_the_last = |runs: &[Range<usize>]| runs.iter().rev().skip(1).map(|run| run.end).collect::<Vec<_>>();
    starts(was) == starts(now) && ends_before_the_last(was) == ends_before_the_last(now)
}

/// Less than this isn't the scrollbar moving the list, only the same place read back and rounded.
const NO_MOVE: Pixels = px(0.01);

#[cfg(test)]
mod tests;
