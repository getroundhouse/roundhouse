//! A copy whose count is checked against one that keeps every row, and the lines they're written.

use std::fmt::Write as _;

use protocol::terminal::Numbering;

use crate::mirror::HISTORY_LINES;
use crate::mirror::rows::counting::Numbered;
use crate::{Mirror, Size};

pub(super) const SIZE: Size = Size { cols: 20, rows: 5 };

/// A copy that never drops a row, against which another's count is checked: the rows that have scrolled into its
/// history are the screen's first row's number in a numbering that started with none.
pub(super) fn reference(size: Size) -> Mirror {
    let mut mirror = Mirror::new(size).unwrap();
    mirror
        .terminal
        .set_scrollback_max_lines(Some(100_000_000))
        .unwrap()
        .set_scrollback_max_bytes(Some(1 << 40))
        .unwrap();
    mirror
}

/// A copy and its reference, written the same, with where the reference's history stood as the copy's numbering began.
pub(super) struct Pair {
    pub(super) copy: Mirror,
    reference: Mirror,
    began: (Numbering, u64, u64),
}

impl Pair {
    /// With the history a terminal keeps. Ghostty drops rows a page at a time, so a smaller cap than a page of rows,
    /// about 1,300 at 20 columns, would lose the count on every write, and test nothing.
    pub(super) fn new(size: Size) -> Self {
        let mut pair = Self {
            copy: Mirror::new(size).unwrap(),
            reference: reference(size),
            began: (Numbering::new(u64::MAX), 0, 0),
        };
        pair.check();
        pair
    }

    /// With its history nearly full, so what's written next drops the oldest rows.
    pub(super) fn nearly_full(size: Size) -> Self {
        let mut pair = Self::new(size);
        pair.write(&lines(0, HISTORY_LINES - 50));
        pair
    }

    pub(super) fn write(&mut self, bytes: &[u8]) -> Numbered {
        self.copy.write(bytes);
        self.reference.write(bytes);
        self.check()
    }

    pub(super) fn resize(&mut self, size: Size) -> Numbered {
        self.copy.resize(size).unwrap();
        self.reference.resize(size).unwrap();
        self.check()
    }

    /// Fails if the copy's numbering is the one it was, and its count has moved by other than the reference's.
    pub(super) fn check(&mut self) -> Numbered {
        let now = self.copy.numbered().unwrap();
        let history = u64::try_from(self.reference.terminal.scrollback_rows().unwrap()).unwrap();
        if !now.shell {
            return now;
        }
        let (numbering, top, reference) = self.began;
        if now.numbering == numbering {
            let moved = history.checked_sub(reference).expect("the reference lost history, and the numbering went on");
            assert_eq!(now.top.get() - top, moved, "the count is wrong");
        } else {
            self.began = (now.numbering, now.top.get(), history);
        }
        now
    }
}

pub(super) fn lines(from: usize, to: usize) -> Vec<u8> {
    let mut written = String::new();
    for n in from..to {
        let _ = write!(written, "line {n}\r\n");
    }
    written.into_bytes()
}
