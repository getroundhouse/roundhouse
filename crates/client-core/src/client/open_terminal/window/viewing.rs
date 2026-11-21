//! A client's window onto a terminal: the rows it holds near its view, where the view is scrolled to, and what's
//! selected, all by row number, from the pictures and replies the hub sends, and what it asks the hub for.

use std::sync::Arc;

use protocol::terminal::{
    Bounds, MOST_ROWS, Numbering, RowNumber, TerminalBoundsReply, TerminalFitReply, TerminalPicture, TerminalPoint,
    TerminalRowsReply, TerminalSize, Typed, Wheel,
};
use terminal_state::Size;
use terminal_state::drawn::{Frame, Rgb, Row, Selected};

use super::fitting::Fitting;
use super::painting::{Colors, GHOSTTYS_COLORS, wide_at};
use super::rows::{Incomplete, Rows, Taken};
use super::selecting::{Selecting, Step};

/// How many screens of history a view holds on each side of it while it's scrolled back, and how many screens it
/// holds above the screen at the bottom, which are the rows that last scrolled off, so they come free.
const SCREENS_AROUND: u64 = 2;
const SCREENS_AT_THE_BOTTOM: u64 = 3;

/// What the window asks of the hub. Each but what's typed waits for a reply, which comes back as a [`Replied`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::client::open_terminal) enum Ask {
    Typed(Typed),
    Rows { numbering: Numbering, from: RowNumber, count: u32 },
    Fit { size: TerminalSize, numbering: Numbering, keep: Vec<TerminalPoint> },
    Bounds { numbering: Numbering, at: TerminalPoint, of: Bounds, step: Step },
}

/// A reply to an [`Ask`].
#[derive(Debug)]
pub(in crate::client::open_terminal) enum Replied {
    Rows(TerminalRowsReply),
    /// To a fit for `size`, which kept the cells `asked` for.
    Fit {
        size: TerminalSize,
        asked: Vec<TerminalPoint>,
        reply: TerminalFitReply,
    },
    Bounds {
        step: Step,
        reply: TerminalBoundsReply,
    },
    /// What was asked failed, or never reached the hub, so it can be asked again.
    Failed,
}

pub(in crate::client::open_terminal) struct Window {
    pub(super) rows: Rows,
    pub(super) colors: Colors,
    pub(super) view_colors: (Rgb, Rgb),
    /// The view's top row while it's scrolled back through the history, and none at the bottom.
    pub(super) scrolled: Option<RowNumber>,
    pub(super) selecting: Selecting,
    /// The rows asked for, until they come.
    pub(super) fetching: bool,
    /// The size the view has room for, once it's laid out.
    pub(super) room: Option<TerminalSize>,
    pub(super) fitting: Option<Fitting>,
    /// The rows the last frame drew, so a row that hasn't changed is the same `Arc`.
    pub(super) drawn: Vec<Arc<Row>>,
    pub(super) changed: bool,
}

impl Window {
    pub(in crate::client::open_terminal) fn new() -> Self {
        let colors = Colors::new(&protocol::terminal::ScreenState::default(), GHOSTTYS_COLORS);
        Self {
            rows: Rows::default(),
            colors,
            view_colors: GHOSTTYS_COLORS,
            scrolled: None,
            selecting: Selecting::default(),
            fetching: false,
            room: None,
            fitting: None,
            drawn: Vec::new(),
            changed: true,
        }
    }

    /// Takes in a frame of a picture. One that finishes a picture in a numbering the window didn't ask a fit for takes
    /// the view to the bottom with nothing selected, since its rows have gone.
    pub(in crate::client::open_terminal) fn take(&mut self, picture: TerminalPicture) -> Result<Vec<Ask>, Incomplete> {
        let fresh = picture.fresh;
        let taken = self.rows.take(picture, &self.colors)?;
        if taken == Taken::Gathered {
            return Ok(Vec::new());
        }
        self.changed = true;
        let screen = self.rows.screen.as_ref().map(|screen| (screen.numbering, screen.size, screen.state.clone()));
        let Some((numbering, size, state)) = screen else { return Ok(Vec::new()) };
        let colors = Colors::new(&state, self.view_colors);
        if colors != self.colors {
            self.colors = colors;
            self.rows.repaint(&self.colors);
        }
        let mut asks = Vec::new();
        match taken {
            Taken::Gathered | Taken::Same => {}
            // A view that watches again, as on a new connection, takes the terminal back to its own size.
            Taken::Fresh => {
                self.fetching = false;
                asks.extend(self.fit_to_room(false));
                asks.extend(self.fetch_above());
            }
            Taken::Anew => {
                self.fetching = false;
                self.anew(numbering, size);
                if fresh {
                    asks.extend(self.fit_to_room(false));
                }
                asks.extend(self.fetch_above());
            }
        }
        self.scrolled = self.scrolled.map(|top| self.clamped(top));
        asks.extend(self.fetch());
        Ok(asks)
    }

    /// Lets go of what it holds of the history, as when the connection goes, since rows on the screen then could
    /// change before they leave it. The screen is drawn until the next picture.
    pub(in crate::client::open_terminal) fn disconnected(&mut self) {
        self.rows.forget();
        self.fetching = false;
        self.fitting = None;
        self.selecting.not_coming();
    }

    pub(in crate::client::open_terminal) fn replied(&mut self, replied: Replied) -> Vec<Ask> {
        match replied {
            Replied::Rows(reply) => {
                self.fetching = false;
                self.rows.came(reply, &self.colors);
                self.changed = true;
                self.fetch()
            }
            Replied::Fit { size, asked, reply } => self.fitted(size, &asked, &reply),
            Replied::Bounds { step, reply } => {
                let numbering = self.rows.screen.as_ref().map(|screen| screen.numbering);
                if numbering == Some(reply.numbering) && self.selecting.bounds(step, reply.range) {
                    self.changed = true;
                }
                Vec::new()
            }
            Replied::Failed => {
                self.fetching = false;
                self.fitting = None;
                self.selecting.not_coming();
                Vec::new()
            }
        }
    }

    /// Takes the view's own colours for a cell that sets none.
    pub(in crate::client::open_terminal) fn set_colors(&mut self, foreground: Rgb, background: Rgb) {
        self.view_colors = (foreground, background);
        let state = self.rows.screen.as_ref().map_or_default(|screen| screen.state.clone());
        self.colors = Colors::new(&state, self.view_colors);
        self.rows.repaint(&self.colors);
        self.changed = true;
    }

    /// What's typed, which also takes the view back to the bottom and clears what's selected, as typing does in
    /// Ghostty. Nothing is sent before the first picture.
    pub(in crate::client::open_terminal) fn typed(&mut self, typed: Typed) -> Vec<Ask> {
        if self.rows.screen.is_none() {
            return Vec::new();
        }
        self.scrolled = None;
        self.selecting.clear();
        self.unkept(true, true);
        self.changed = true;
        vec![Ask::Typed(typed)]
    }

    /// Turns the wheel by `lines`, up for fewer than none, which scrolls the view through the history, or is sent to
    /// a full-screen program, as the picture says.
    pub(in crate::client::open_terminal) fn wheel(&mut self, lines: isize) -> Vec<Ask> {
        if self.waiting() {
            return Vec::new();
        }
        let Some(screen) = &self.rows.screen else { return Vec::new() };
        match screen.state.wheel {
            Wheel::Arrows => {
                let lines = i32::try_from(lines).unwrap_or(if lines < 0 { i32::MIN } else { i32::MAX });
                return vec![Ask::Typed(Typed::Wheel { lines })];
            }
            Wheel::Program => return Vec::new(),
            Wheel::Scrolls | Wheel::Unknown => {}
        }
        let top = self.scrolled.unwrap_or(screen.top).get();
        let by = u64::try_from(lines.unsigned_abs()).unwrap_or(u64::MAX);
        let moved = if lines < 0 { top.saturating_sub(by) } else { top.saturating_add(by) };
        let bottom = screen.top;
        self.unkept(true, false);
        self.scrolled = Some(self.clamped(RowNumber::new(moved))).filter(|top| *top < bottom);
        self.changed = true;
        self.fetch()
    }

    /// `top` as a top the view can have, from the oldest row kept to the screen's top.
    pub(super) fn clamped(&self, top: RowNumber) -> RowNumber {
        self.rows.screen.as_ref().map_or(top, |screen| top.clamp(screen.first, screen.top))
    }

    /// The view's top row.
    pub(super) fn top(&self) -> Option<RowNumber> {
        let screen = self.rows.screen.as_ref()?;
        Some(self.scrolled.unwrap_or(screen.top))
    }

    /// The two screens of history above the screen, once, as the view first shows it, so a small scroll has them.
    fn fetch_above(&mut self) -> Vec<Ask> {
        let Some(screen) = self.rows.screen.as_ref().filter(|_| self.scrolled.is_none() && !self.fetching) else {
            return Vec::new();
        };
        let height = u64::from(screen.size.rows);
        let from = RowNumber::new(screen.top.get().saturating_sub(SCREENS_AROUND * height)).max(screen.first);
        if from >= screen.top {
            return Vec::new();
        }
        self.fetching = true;
        let count = u32::try_from(screen.top.get() - from.get()).unwrap_or(MOST_ROWS).min(MOST_ROWS);
        vec![Ask::Rows { numbering: screen.numbering, from, count }]
    }

    /// Rows of history the view lacks near it, nearest first, unless some are being fetched already. It holds the rows
    /// from two screens above its top to two below its end, and at the bottom, the three screens above it.
    pub(super) fn fetch(&mut self) -> Vec<Ask> {
        let (Some(top), Some(screen)) = (self.top(), &self.rows.screen) else { return Vec::new() };
        let (height, numbering, first, screen_top) =
            (u64::from(screen.size.rows), screen.numbering, screen.first, screen.top);
        if self.scrolled.is_none() {
            self.rows.let_go_before(RowNumber::new(top.get().saturating_sub(SCREENS_AT_THE_BOTTOM * height)));
            return Vec::new();
        }
        self.rows.let_go_far_from(top);
        if self.fetching {
            return Vec::new();
        }
        let from = RowNumber::new(top.get().saturating_sub(SCREENS_AROUND * height)).max(first);
        let to = top.plus((1 + SCREENS_AROUND) * height).min(screen_top);
        // The rows in sight first, then those above and below, by how far they are from the view, a reply's worth
        // at a time, going away from it.
        let lacking = (from.get()..to.get()).map(RowNumber::new).filter(|number| !self.rows.has(*number));
        let Some(nearest) = lacking.min_by_key(|number| distance(*number, top, height)) else { return Vec::new() };
        let most = u64::from(MOST_ROWS);
        let (start, count) = if nearest < top {
            let start = RowNumber::new(nearest.get().saturating_sub(most - 1)).max(from);
            (start, nearest.get() - start.get() + 1)
        } else {
            (nearest, (to.get() - nearest.get()).min(most))
        };
        self.fetching = true;
        let count = u32::try_from(count).unwrap_or(MOST_ROWS);
        vec![Ask::Rows { numbering, from: start, count }]
    }

    /// What the view shows, if that has changed since the last frame. A row it lacks is blank, in the terminal's own
    /// colours, until it comes.
    pub(in crate::client::open_terminal) fn frame(&mut self) -> Option<Arc<Frame>> {
        if self.waiting() || !std::mem::take(&mut self.changed) {
            return None;
        }
        let top = self.top()?;
        let screen = self.rows.screen.as_ref()?;
        let size = Size { cols: screen.size.cols, rows: screen.size.rows };
        let blank = Arc::new(Row::default());
        let rows: Vec<Arc<Row>> = (0..u64::from(size.rows))
            .map(|at| top.plus(at))
            .map(|number| {
                let painted = self.rows.row(number).map_or_else(|| blank.clone(), |held| held.painted.clone());
                match self.selecting.cells(number, size.cols) {
                    Some((col, end)) => {
                        // Off a wide character's second cell at the start, and onto it at the end, as Ghostty draws it.
                        let col = col - u16::from(col > 0 && wide_at(&painted, col - 1));
                        let end = end + u16::from(wide_at(&painted, end));
                        let selected = Selected { col, cells: end.saturating_sub(col) + 1 };
                        Arc::new(Row { selected: Some(selected), ..(*painted).clone() })
                    }
                    None => painted,
                }
            })
            .collect();
        // A row that's the same as last time is the same `Arc`, so a view can tell.
        let rows = rows
            .into_iter()
            .enumerate()
            .map(|(at, row)| match self.drawn.get(at) {
                Some(was) if Arc::ptr_eq(was, &row) || **was == *row => was.clone(),
                Some(_) | None => row,
            })
            .collect::<Vec<_>>();
        self.drawn.clone_from(&rows);
        let cursor = screen.cursor.and_then(|cursor| {
            let on = screen.top.plus(u64::from(cursor.row));
            let row = u16::try_from(on.get().checked_sub(top.get())?).ok().filter(|row| *row < size.rows)?;
            Some(self.colors.cursor(cursor, row, &rows[usize::from(row)]))
        });
        let (foreground, background) = (self.colors.foreground, self.colors.background);
        Some(Arc::new(Frame { size, rows, cursor, foreground, background }))
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
