//! Which rows of history a window asks for: those in sight, two screens behind the view, and a lead ahead of it, in the
//! way it last moved, several asks at once, so a trackpad's fling finds its rows there, as #2129 measured.

use std::ops::Range;

use protocol::terminal::{MOST_ROWS, Numbering, RowNumber};

use super::viewing::{Ask, Window};

/// How many screens of history a view holds behind it while it's scrolled back, which it also asks for above the screen
/// as it's first shown, and how many it holds above the screen at the bottom, which are the rows that last scrolled off,
/// so they come free.
const SCREENS_BEHIND: u64 = 2;
const SCREENS_AT_THE_BOTTOM: u64 = 3;

/// How many rows a view holds ahead of it while it's scrolled back: at least this many, and at least so many screens,
/// as the owner chose from replayed flings, which peaked at 3,000 to 12,500 lines a second. It's at most half the rows a
/// window has room for, so what's in sight and behind it fit beside it with room to spare: 1,000 rows, which a terminal
/// over 100 rows tall reaches before ten screens, and fewer where rows are so large that 2,000 would take too many
/// bytes. A lead that nears what's held has the window let go of rows it then asks for again.
const LEAD_ROWS: u64 = 600;
const LEAD_SCREENS: u64 = 10;

/// How many asks for rows can wait for their replies at once.
const MOST_FETCHING: usize = 4;

/// Which way the view last moved through the history, which the lead goes in. Up, before it has moved, since from the
/// bottom that's the only way to go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Heading {
    #[default]
    Up,
    Down,
}

/// The runs of rows asked for whose replies haven't come, each in the numbering it was asked in, from the first row
/// asked for. Each run is let go of once its reply comes, whatever it brought, since what it lacks is asked for again.
#[derive(Debug, Default)]
pub(super) struct Fetching {
    runs: Vec<(Numbering, Range<RowNumber>)>,
}

impl Fetching {
    fn has(&self, number: RowNumber) -> bool {
        self.runs.iter().any(|(_, run)| run.contains(&number))
    }

    /// The reply to the run asked for in `numbering` from `from` has come. A reply says where its rows start, which is
    /// later than what was asked when the oldest rows have gone meanwhile, so it's matched by what was asked.
    pub(super) fn replied(&mut self, numbering: Numbering, from: RowNumber) {
        self.runs.retain(|(asked, run)| (*asked, run.start) != (numbering, from));
    }

    /// Nothing asked for is waited for any more, as when a picture's rows replace those held, or the connection goes.
    pub(super) fn clear(&mut self) {
        self.runs.clear();
    }

    /// Asks for the rows of `run`, as many as one reply brings.
    fn ask(&mut self, numbering: Numbering, run: Range<RowNumber>) -> Ask {
        let count = u32::try_from(run.end.get() - run.start.get()).unwrap_or(MOST_ROWS).min(MOST_ROWS);
        let from = run.start;
        self.runs.push((numbering, from..from.plus(u64::from(count))));
        Ask::Rows { numbering, from, count }
    }
}

impl Window {
    /// The two screens of history above the screen, once, as the view first shows it, so a small scroll has them.
    pub(super) fn fetch_above(&mut self) -> Vec<Ask> {
        let Some(screen) =
            self.rows.screen.as_ref().filter(|_| self.scrolled.is_none() && self.fetching.runs.is_empty())
        else {
            return Vec::new();
        };
        let height = u64::from(screen.size.rows);
        let from = RowNumber::new(screen.top.get().saturating_sub(SCREENS_BEHIND * height)).max(screen.first);
        if from >= screen.top {
            return Vec::new();
        }
        vec![self.fetching.ask(screen.numbering, from..screen.top)]
    }

    /// Rows of history the view lacks near it, as many runs as can wait at once, nearest first, so the hub sends those
    /// in sight first. Scrolled back, it holds those in sight, two screens behind, and the lead ahead, and lets go of
    /// others first. At the bottom, it holds the three screens above it.
    pub(super) fn fetch(&mut self) -> Vec<Ask> {
        let (Some(top), Some(screen)) = (self.top(), &self.rows.screen) else { return Vec::new() };
        let (height, numbering, first, screen_top) =
            (u64::from(screen.size.rows), screen.numbering, screen.first, screen.top);
        if self.scrolled.is_none() {
            self.rows.let_go_before(RowNumber::new(top.get().saturating_sub(SCREENS_AT_THE_BOTTOM * height)));
            return Vec::new();
        }
        let (behind, lead) =
            (SCREENS_BEHIND * height, (LEAD_SCREENS * height).max(LEAD_ROWS).min(self.rows.room() / 2));
        let (above, below) = match self.heading {
            Heading::Up => (lead, behind),
            Heading::Down => (behind, lead),
        };
        let wanted =
            RowNumber::new(top.get().saturating_sub(above)).max(first)..top.plus(height + below).min(screen_top);
        let wanted = self.rows.let_go_outside(wanted, top);
        let mut asks = Vec::new();
        while self.fetching.runs.len() < MOST_FETCHING {
            let Some(run) = self.lacking(&wanted, top, height) else { break };
            asks.push(self.fetching.ask(numbering, run));
        }
        asks
    }

    /// The run of rows of `wanted` nearest the view that are neither held nor asked for, the rows in sight first, then
    /// those above and below by how far they are from it, a reply's worth at most, going away from it.
    fn lacking(&self, wanted: &Range<RowNumber>, top: RowNumber, height: u64) -> Option<Range<RowNumber>> {
        let lacks = |number: &u64| {
            let number = RowNumber::new(*number);
            !self.rows.has(number) && !self.fetching.has(number)
        };
        let (start, end) = (wanted.start.get(), wanted.end.get());
        let nearest = (start..end).filter(lacks).min_by_key(|number| distance(RowNumber::new(*number), top, height))?;
        let most = usize::try_from(MOST_ROWS).unwrap_or(usize::MAX);
        let run = if nearest < top.get() {
            let from = (start..=nearest).rev().take(most).take_while(lacks).last().unwrap_or(nearest);
            from..nearest + 1
        } else {
            let last = (nearest..end).take(most).take_while(lacks).last().unwrap_or(nearest);
            nearest..last + 1
        };
        Some(RowNumber::new(run.start)..RowNumber::new(run.end))
    }
}

/// How far row `number` is from a view of `height` rows from `top`, none for a row in sight.
fn distance(number: RowNumber, top: RowNumber, height: u64) -> u64 {
    let end = top.plus(height);
    if number < top {
        top.get() - number.get()
    } else if number >= end {
        number.get() - end.get() + 1
    } else {
        0
    }
}
