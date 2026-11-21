//! A window fed the pictures of a worker's copy of a terminal, as a hub would send them.

use std::fmt::Write as _;

use protocol::terminal::{PictureSending, TerminalPicture};
use terminal_state::drawn::Rgb;
use terminal_state::{Mirror, Size};

use super::super::{Ask, Replied, Window};

pub(super) const FOREGROUND: Rgb = Rgb { r: 0xe0, g: 0xe0, b: 0xd0 };
pub(super) const BACKGROUND: Rgb = Rgb { r: 0x10, g: 0x12, b: 0x14 };

/// A worker's copy of a terminal of `size` that `output` was written to.
pub(super) fn worker(size: Size, output: &[u8]) -> Mirror {
    let mut mirror = Mirror::new(size).unwrap();
    mirror.write(output);
    mirror
}

/// The frames of the picture a worker sends next of what `worker` shows, as `sending` says it lacks.
pub(super) fn pictured(worker: &mut Mirror, sending: &mut PictureSending) -> Vec<TerminalPicture> {
    let shown = worker.shown().unwrap();
    let mut frames = Vec::new();
    let mut next = sending.next(shown, |number| worker.history_row(number));
    while let Some(picture) = next {
        frames.push(picture);
        next = sending.rest();
    }
    frames
}

/// A window in the view's colours, shown the next picture of what `worker` shows, with the rows it asks for as it
/// first shows it, as a hub would answer.
pub(super) fn watched(worker: &mut Mirror, sending: &mut PictureSending) -> Window {
    let mut window = Window::new();
    window.set_colors(FOREGROUND, BACKGROUND);
    let mut asks = Vec::new();
    for frame in pictured(worker, sending) {
        asks.extend(window.take(frame).unwrap());
    }
    while let Some(ask) = asks.pop() {
        let Ask::Rows { numbering, from, count } = ask else { panic!("asked for {ask:?}") };
        asks.extend(window.replied(Replied::Rows(worker.history_rows(numbering, from, count).unwrap())));
    }
    window
}

/// Lines numbered by `numbers`, each ended as a program ends one.
pub(super) fn lines(numbers: std::ops::Range<usize>) -> String {
    numbers.fold(String::new(), |mut lines, at| {
        let _ = write!(lines, "line {at}\r\n");
        lines
    })
}
