//! The rows a client holds of a terminal: the screen of the newest whole picture, and rows of history near where the
//! view is, by number, as `protocol::terminal::TerminalPicture` says a receiver keeps them.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::Arc;

use protocol::terminal::{
    Numbering, RowNumber, ScreenState, TerminalCursor, TerminalPicture, TerminalRow, TerminalRowsReply, TerminalSize,
};

use super::painting::Colors;
use crate::drawn::Row;

/// The most rows of history a client holds, and the most bytes of them, letting go first of those furthest from the
/// rows its view wants.
pub(super) const MOST_HELD_ROWS: usize = 2_000;
pub(super) const MOST_HELD_BYTES: usize = 8 * 1024 * 1024;

/// A row, as it came, and as it's drawn.
#[derive(Clone, Debug)]
pub(super) struct Held {
    pub(super) row: Arc<TerminalRow>,
    pub(super) painted: Arc<Row>,
    /// About how much it takes, as JSON.
    bytes: usize,
}

impl Held {
    fn new(row: TerminalRow, bytes: usize, colors: &Colors) -> Self {
        let painted = colors.paint(&row);
        Self { row: Arc::new(row), painted, bytes }
    }
}

/// The screen of the newest whole picture.
#[derive(Clone, Debug)]
pub(super) struct Screen {
    pub(super) numbering: Numbering,
    pub(super) size: TerminalSize,
    pub(super) top: RowNumber,
    pub(super) first: RowNumber,
    pub(super) rows: Vec<Held>,
    pub(super) cursor: Option<TerminalCursor>,
    pub(super) state: ScreenState,
}

impl Screen {
    fn held(&self, number: RowNumber) -> Option<&Held> {
        let at = usize::try_from(number.get().checked_sub(self.top.get())?).ok()?;
        self.rows.get(at)
    }

    /// The number after the screen's last row.
    pub(super) fn end(&self) -> RowNumber {
        self.top.plus(self.rows.len() as u64)
    }
}

/// What taking in a frame did.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Taken {
    /// Only gathered it, as a frame before a picture's last.
    Gathered,
    /// It finished a picture in the numbering and at the size of the one before.
    Same,
    /// It finished one sent from nothing, as after a new connection, in the numbering of the one before, so the rows
    /// held from before have gone, but a place by number holds.
    Fresh,
    /// It finished one in another numbering, so nothing held from before is.
    Anew,
}

/// A picture lacked a row the client doesn't hold, which a sender that knows what its receiver holds never sends, so
/// the client watches again from nothing.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::client::open_terminal) struct Incomplete;

/// The frames of a picture whose last hasn't come.
#[derive(Debug)]
struct Gathering {
    numbering: Numbering,
    size: TerminalSize,
    top: RowNumber,
    fresh: bool,
    rows: HashMap<RowNumber, Held>,
}

impl Gathering {
    fn of(picture: &TerminalPicture) -> Self {
        let (numbering, size, top, fresh) = (picture.numbering, picture.size, picture.top, picture.fresh);
        Self { numbering, size, top, fresh, rows: HashMap::new() }
    }

    fn is_of(&self, picture: &TerminalPicture) -> bool {
        (self.numbering, self.size, self.top, self.fresh)
            == (picture.numbering, picture.size, picture.top, picture.fresh)
    }
}

#[derive(Debug, Default)]
pub(super) struct Rows {
    gathering: Option<Gathering>,
    pub(super) screen: Option<Screen>,
    /// Rows above the screen, in its numbering.
    history: BTreeMap<RowNumber, Held>,
    bytes: usize,
}

impl Rows {
    /// The row with `number` in the screen's numbering, if it's held.
    pub(super) fn row(&self, number: RowNumber) -> Option<&Held> {
        let screen = self.screen.as_ref()?;
        if number >= screen.top { screen.held(number) } else { self.history.get(&number) }
    }

    /// Lets go of everything, as when the connection goes, after which a whole picture comes. The screen stays to be
    /// drawn until then.
    pub(super) fn forget(&mut self) {
        self.gathering = None;
        self.clear_history();
    }

    /// Takes in a frame of a picture, and on its last makes it what's shown.
    pub(super) fn take(&mut self, picture: TerminalPicture, colors: &Colors) -> Result<Taken, Incomplete> {
        let mut gathering = match self.gathering.take() {
            Some(gathering) if gathering.is_of(&picture) => gathering,
            Some(_) | None => Gathering::of(&picture),
        };
        let TerminalPicture { rows, whole, kept_from, first, cursor, state, .. } = picture;
        for run in rows {
            for (number, json) in (run.from.get()..).map(RowNumber::new).zip(run.rows) {
                gathering.rows.insert(number, Held::new(json.row(), json.len(), colors));
            }
        }
        if !whole {
            self.gathering = Some(gathering);
            return Ok(Taken::Gathered);
        }
        let before = self.screen.take();
        let same = before.as_ref().filter(|before| {
            !gathering.fresh && (before.numbering, before.size) == (gathering.numbering, gathering.size)
        });
        let rows = (0..u64::from(gathering.size.rows))
            .map(|at| gathering.top.plus(at))
            .map(|number| gathering.rows.remove(&number).or_else(|| same.and_then(|same| same.held(number)).cloned()))
            .collect::<Option<Vec<_>>>();
        let state = state.or_else(|| same.map(|same| same.state.clone()));
        let (Some(rows), Some(state)) = (rows, state) else {
            self.screen = before;
            self.forget();
            return Err(Incomplete);
        };
        let screen = Screen {
            numbering: gathering.numbering,
            size: gathering.size,
            top: gathering.top,
            first,
            rows,
            cursor,
            state,
        };
        let numbered_alike =
            before.as_ref().is_some_and(|before| (before.numbering, before.size) == (screen.numbering, screen.size));
        let taken = match (same.is_some(), numbered_alike) {
            (true, _) => Taken::Same,
            (false, true) => Taken::Fresh,
            (false, false) => Taken::Anew,
        };
        self.keep_what_left(same, &screen, kept_from);
        self.screen = Some(screen);
        Ok(taken)
    }

    /// Holds the rows of `before` that have left the screen as of `now`, from `kept_from` on, which the sender says
    /// are history as it is, and lets go of those held before `kept_from`. With no screen before in the same numbering,
    /// nothing held is.
    fn keep_what_left(&mut self, before: Option<&Screen>, now: &Screen, kept_from: RowNumber) {
        match before {
            Some(before) => {
                let left = before.rows.iter().enumerate().map(|(at, held)| (before.top.plus(at as u64), held));
                let left: Vec<_> = left.filter(|(number, _)| *number < now.top && *number >= kept_from).collect();
                for (number, held) in left {
                    self.hold(number, held.clone());
                }
            }
            None => self.clear_history(),
        }
        self.let_go_before(kept_from.max(now.first));
    }

    /// Takes in rows of history that were asked for, if they're in the numbering the screen is in.
    pub(super) fn came(&mut self, reply: TerminalRowsReply, colors: &Colors) {
        let Some(screen) = &self.screen else { return };
        if reply.numbering != screen.numbering {
            return;
        }
        let (first, top) = (screen.first, screen.top);
        for (number, row) in (reply.from.get()..).map(RowNumber::new).zip(reply.rows) {
            if number >= first && number < top {
                let bytes = serde_json::to_vec(&row).map_or(0, |json| json.len());
                self.hold(number, Held::new(row, bytes, colors));
            }
        }
    }

    /// Whether row `number` of history is held, or is the screen's.
    pub(super) fn has(&self, number: RowNumber) -> bool {
        self.row(number).is_some()
    }

    /// About how many rows of history it has room for: the most it holds, or fewer, where rows the size of those held
    /// would take more than the most bytes before that.
    pub(super) fn room(&self) -> u64 {
        let room = match self.history.len() {
            0 => MOST_HELD_ROWS,
            held => MOST_HELD_ROWS.min(MOST_HELD_BYTES / (self.bytes / held).max(1)),
        };
        u64::try_from(room).unwrap_or(u64::MAX)
    }

    /// Lets go of rows of history until no more than the most are held: those furthest outside `wanted` first, and if
    /// what's wanted is more than that, those at the end of it furthest from `near`, the view's top. Returns what of
    /// `wanted` it still has room for, which is all of it unless it let go of rows in it, so those aren't asked for
    /// again at once.
    pub(super) fn let_go_outside(&mut self, mut wanted: Range<RowNumber>, near: RowNumber) -> Range<RowNumber> {
        while self.history.len() > MOST_HELD_ROWS || self.bytes > MOST_HELD_BYTES {
            let (Some((&oldest, _)), Some((&newest, _))) =
                (self.history.first_key_value(), self.history.last_key_value())
            else {
                break;
            };
            let before = wanted.start.get().saturating_sub(oldest.get());
            let after = newest.plus(1).get().saturating_sub(wanted.end.get());
            let far = if before > 0 || after > 0 {
                if before >= after { oldest } else { newest }
            } else if near.get().abs_diff(oldest.get()) >= near.get().abs_diff(newest.get()) {
                wanted.start = oldest.plus(1);
                oldest
            } else {
                wanted.end = newest;
                newest
            };
            if let Some(gone) = self.history.remove(&far) {
                self.bytes -= gone.bytes;
            }
        }
        wanted
    }

    /// Draws every row again in `colors`, as after the view's own change.
    pub(super) fn repaint(&mut self, colors: &Colors) {
        let rows = self.history.values_mut().chain(self.screen.iter_mut().flat_map(|screen| screen.rows.iter_mut()));
        for held in rows {
            held.painted = colors.paint(&held.row);
        }
        for held in self.gathering.iter_mut().flat_map(|gathering| gathering.rows.values_mut()) {
            held.painted = colors.paint(&held.row);
        }
    }

    fn hold(&mut self, number: RowNumber, held: Held) {
        self.bytes += held.bytes;
        if let Some(was) = self.history.insert(number, held) {
            self.bytes -= was.bytes;
        }
    }

    /// Lets go of the rows of history before `number`.
    pub(super) fn let_go_before(&mut self, number: RowNumber) {
        let kept = self.history.split_off(&number);
        let gone = std::mem::replace(&mut self.history, kept);
        self.bytes -= gone.values().map(|held| held.bytes).sum::<usize>();
    }

    fn clear_history(&mut self) {
        self.history.clear();
        self.bytes = 0;
    }
}
