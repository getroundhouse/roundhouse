//! What a sender of a terminal's pictures keeps of what its receiver has, and the next frame it sends it. The worker
//! is one, for its hub, and the hub is one for each client, with the same code, so the worker never hears of a client.
//!
//! There are no acknowledgements. A connection is in order and loses nothing, so what a receiver has is what was sent.

use std::collections::VecDeque;

use super::picture::{RowJson, RowRun, ScreenState, TerminalCursor, TerminalPicture};
use super::rows::{Numbering, RowNumber};
use super::size::TerminalSize;

/// The rows a frame carries at most, as JSON, or one row if that's more, so what's sent behind it waits for little.
pub const MOST_PICTURE_BYTES: usize = 64 * 1024;

/// What a terminal shows now, as its sender reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    pub numbering: Numbering,
    pub size: TerminalSize,
    pub top: RowNumber,
    pub first: RowNumber,
    /// The screen's rows, its first first.
    pub rows: Vec<RowJson>,
    pub cursor: Option<TerminalCursor>,
    pub state: ScreenState,
}

/// What a sender keeps for one receiver.
#[derive(Debug, Default)]
pub struct PictureSending {
    /// What the receiver has, as of the last whole picture it was sent.
    has: Option<Has>,
    /// The rest of a picture begun and not finished.
    begun: Option<Begun>,
}

#[derive(Debug)]
struct Has {
    shown: Shown,
    kept_from: RowNumber,
}

#[derive(Debug)]
struct Begun {
    /// What each of its frames says, with no rows.
    frame: TerminalPicture,
    rest: VecDeque<(RowNumber, RowJson)>,
    cursor: Option<TerminalCursor>,
    state: Option<ScreenState>,
    /// What the receiver has once the last frame is sent.
    then: Has,
}

impl PictureSending {
    /// As for a receiver that has nothing, as one that has just begun to watch.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Whether it's partway through a picture, the rest of which it sends before it reads the terminal again.
    #[must_use]
    pub const fn sending(&self) -> bool {
        self.begun.is_some()
    }

    /// The next frame of the picture it began, as the terminal was when it began, whatever it shows now. Otherwise a
    /// large screen under a flood would never be sent whole, since every row would be new again by the next frame.
    pub fn rest(&mut self) -> Option<TerminalPicture> {
        let mut begun = self.begun.take()?;
        let frame = take_frame(&mut begun.rest, begun.frame.clone());
        if begun.rest.is_empty() {
            self.has = Some(begun.then);
            return Some(TerminalPicture { whole: true, cursor: begun.cursor, state: begun.state, ..frame });
        }
        self.begun = Some(begun);
        Some(frame)
    }

    /// The first frame of a picture of what the terminal shows `now`, with the rows the receiver lacks, or `None` if it
    /// has it all. `history` is a row of history by number, while the terminal keeps it.
    pub fn next(
        &mut self,
        now: Shown,
        mut history: impl FnMut(RowNumber) -> Option<RowJson>,
    ) -> Option<TerminalPicture> {
        if self.sending() {
            return self.rest();
        }
        // The same numbering and size, and a top that hasn't gone back, which a numbering never does.
        let had = self.has.as_ref().filter(|has| {
            let shown = &has.shown;
            (shown.numbering, shown.size) == (now.numbering, now.size) && shown.top <= now.top
        });
        let kept_from = had.map_or(now.first, |has| kept_from(has, now.top, &mut history));
        let lacking: VecDeque<_> = (0u64..)
            .zip(&now.rows)
            .map(|(at, row)| (now.top.plus(at), row))
            .filter(|(number, row)| had.and_then(|has| held(&has.shown, *number)) != Some(*row))
            .map(|(number, row)| (number, row.clone()))
            .collect();
        let state = (had.map(|has| &has.shown.state) != Some(&now.state)).then(|| now.state.clone());
        let unchanged = had.is_some_and(|has| {
            let shown = &has.shown;
            (shown.top, shown.first, &shown.cursor, has.kept_from) == (now.top, now.first, &now.cursor, kept_from)
        });
        if lacking.is_empty() && state.is_none() && unchanged {
            return None;
        }
        let frame = TerminalPicture {
            numbering: now.numbering,
            size: now.size,
            top: now.top,
            first: now.first,
            rows: Vec::new(),
            kept_from,
            whole: false,
            cursor: None,
            state: None,
        };
        let cursor = now.cursor;
        self.begun = Some(Begun { frame, rest: lacking, cursor, state, then: Has { shown: now, kept_from } });
        self.rest()
    }
}

/// The receiver's copy of row `number`, if it was on the screen it was sent.
fn held(shown: &Shown, number: RowNumber) -> Option<&RowJson> {
    let at = number.get().checked_sub(shown.top.get())?;
    shown.rows.get(usize::try_from(at).ok()?)
}

/// The first row from which the rows the receiver holds above the screen are as the terminal keeps them, now the
/// screen's top is `top`. A row that left the screen since the receiver's copy is checked against history, and one the
/// receiver never had moves it past.
fn kept_from(has: &Has, top: RowNumber, history: &mut impl FnMut(RowNumber) -> Option<RowJson>) -> RowNumber {
    let held_to = has.shown.top.plus(has.shown.rows.len() as u64);
    if top > held_to {
        return top;
    }
    let mut kept_from = has.kept_from;
    for number in has.shown.top.get()..top.get() {
        let number = RowNumber::new(number);
        let same = held(&has.shown, number).is_some_and(|had| history(number).as_ref() == Some(had));
        if !same {
            kept_from = number.plus(1);
        }
    }
    kept_from
}

/// A frame of `frame`'s with the next rows of `rest`, at most [`MOST_PICTURE_BYTES`] of them, or one.
fn take_frame(rest: &mut VecDeque<(RowNumber, RowJson)>, mut frame: TerminalPicture) -> TerminalPicture {
    let mut bytes = 0;
    while let Some((number, row)) = rest.front() {
        if bytes > 0 && bytes + row.len() > MOST_PICTURE_BYTES {
            break;
        }
        bytes += row.len();
        let follows = frame.rows.last().is_some_and(|run| run.from.plus(run.rows.len() as u64) == *number);
        let row = row.clone();
        match frame.rows.last_mut().filter(|_| follows) {
            Some(run) => run.rows.push(row),
            None => frame.rows.push(RowRun { from: *number, rows: vec![row] }),
        }
        rest.pop_front();
    }
    frame
}
