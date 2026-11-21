//! Counting the rows that scroll into a terminal's history, which Ghostty doesn't, from a tracked reference on the
//! newest row of history, which follows that row as rows scroll and old ones are dropped.

use libghostty_vt::Terminal;
use libghostty_vt::grid_ref::TrackedGridRef;
use libghostty_vt::point::{Coordinate, Point, Space};
use libghostty_vt::screen::Screen;
use protocol::terminal::{Numbering, RowNumber};

use crate::MirrorError;

/// The numbering the screen showing is in, and where its rows are in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::mirror) struct Numbered {
    pub(in crate::mirror) numbering: Numbering,
    /// The oldest row of history the terminal keeps.
    pub(in crate::mirror) first: RowNumber,
    /// The screen's first row.
    pub(in crate::mirror) top: RowNumber,
    /// Whether it's the shell's screen, which has the history, rather than a full-screen program's.
    pub(in crate::mirror) shell: bool,
}

/// The numberings of a terminal's two screens. Made only for the worker's copy, the first time its rows are asked for,
/// since only the worker's are, and a numbering can start whenever it likes.
#[derive(Debug)]
pub(in crate::mirror) struct Counting {
    /// The next numbering to start.
    next: Numbering,
    /// The shell's, while the shell's screen shows or since it last did. None until it first does, since a reference
    /// is made on the screen showing.
    shell: Option<Count>,
    /// The program's, while a full-screen program is up. Its screen has no history, so its rows are numbered from its
    /// first.
    program: Option<Numbering>,
    /// The screen showing's, as of the last write.
    now: Numbered,
}

#[derive(Debug)]
struct Count {
    numbering: Numbering,
    /// On the newest row of history, or the screen's first row while there's no history.
    mark: TrackedGridRef,
    /// The number of the row the mark is on.
    marked: RowNumber,
    top: RowNumber,
}

impl Counting {
    pub(in crate::mirror) fn new(terminal: &mut Terminal<'_, '_>) -> Result<Self, MirrorError> {
        let start = RowNumber::new(0);
        let now = Numbered { numbering: Numbering::new(0), first: start, top: start, shell: false };
        let mut counting = Self { next: Numbering::new(0), shell: None, program: None, now };
        counting.written(terminal)?;
        Ok(counting)
    }

    /// Takes in what was just written, which may have scrolled the shell's screen, cleared its history or switched
    /// screens.
    pub(in crate::mirror) fn written(&mut self, terminal: &mut Terminal<'_, '_>) -> Result<(), MirrorError> {
        if terminal.active_screen()? == Screen::Alternate {
            let numbering = if let Some(numbering) = self.program {
                numbering
            } else {
                let numbering = self.start();
                self.program = Some(numbering);
                numbering
            };
            let start = RowNumber::new(0);
            self.now = Numbered { numbering, first: start, top: start, shell: false };
            return Ok(());
        }
        self.program = None;
        let counted = match &mut self.shell {
            Some(count) => count.written(terminal)?,
            None => false,
        };
        if !counted {
            let numbering = self.start();
            self.shell = Some(Count::start(terminal, numbering)?);
        }
        // Made just above, if there wasn't one.
        let Some(count) = &self.shell else { return Ok(()) };
        let first = RowNumber::new(count.top.get().saturating_sub(history(terminal)?));
        self.now = Numbered { numbering: count.numbering, first, top: count.top, shell: true };
        Ok(())
    }

    /// Starts every numbering again, since a terminal of a new size has wrapped every line again.
    pub(in crate::mirror) fn resized(&mut self, terminal: &mut Terminal<'_, '_>) -> Result<(), MirrorError> {
        self.shell = None;
        self.program = None;
        self.written(terminal)
    }

    /// The screen showing's numbering, and where its rows are, as of the last write.
    pub(in crate::mirror) fn now(&self) -> Numbered {
        self.now
    }

    fn start(&mut self) -> Numbering {
        let numbering = self.next;
        self.next = numbering.next();
        numbering
    }
}

impl Count {
    /// From the rows the terminal has now, the oldest of its history numbered 0.
    fn start(terminal: &mut Terminal<'_, '_>, numbering: Numbering) -> Result<Self, MirrorError> {
        let mark = terminal.track_grid_ref(Point::Screen(Coordinate { x: 0, y: 0 }))?;
        let mut count = Self { numbering, mark, marked: RowNumber::new(0), top: RowNumber::new(0) };
        let history = history(terminal)?;
        count.top = RowNumber::new(history);
        count.mark_newest(terminal, history)?;
        Ok(count)
    }

    /// Moves `top` on by the rows that scrolled into history. False if it can't tell how many, as when the mark's row
    /// was dropped or cleared, or rows were put above it, after which no number can be trusted.
    fn written(&mut self, terminal: &mut Terminal<'_, '_>) -> Result<bool, MirrorError> {
        let Some(at) = self.mark.point(Space::Screen)? else { return Ok(false) };
        let history = history(terminal)?;
        // The mark's row is `history - at.y` rows above the screen's first, or the first itself.
        let top = (self.marked.get() + history).checked_sub(u64::from(at.y)).map(RowNumber::new);
        let Some(top) = top.filter(|top| *top >= self.top) else { return Ok(false) };
        self.top = top;
        self.mark_newest(terminal, history)?;
        Ok(true)
    }

    fn mark_newest(&mut self, terminal: &mut Terminal<'_, '_>, history: u64) -> Result<(), MirrorError> {
        let (y, marked) = match history.checked_sub(1) {
            Some(newest) => (newest, RowNumber::new(self.top.get() - 1)),
            None => (0, self.top),
        };
        let y = u32::try_from(y).unwrap_or(u32::MAX);
        self.mark.set(terminal, Point::Screen(Coordinate { x: 0, y }))?;
        self.marked = marked;
        Ok(())
    }
}

fn history(terminal: &Terminal<'_, '_>) -> Result<u64, MirrorError> {
    Ok(u64::try_from(terminal.scrollback_rows()?).unwrap_or(u64::MAX))
}
