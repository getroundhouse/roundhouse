//! Row numbers, which never name another row, however the terminal's rows move.

use std::collections::HashMap;

use libghostty_vt::terminal::CompressionMode;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use protocol::terminal::{Numbering, RowNumber, TerminalRow};

use crate::Size;
use crate::mirror::HISTORY_LINES;
use crate::mirror::rows::reading::{Place, row};

use super::pair::{Pair, SIZE, lines};

/// One line at a time, past where the oldest are dropped, and in writes of many lines, with the history compressed
/// between.
#[test]
fn rows_are_counted_as_they_scroll_into_history_and_out_of_it() {
    let mut pair = Pair::new(SIZE);
    let began = pair.check().numbering;

    for n in 0..HISTORY_LINES + 500 {
        pair.write(&lines(n, n + 1));
    }
    let more = HISTORY_LINES + 500;
    pair.write(&lines(more, more + 2_000));
    pair.copy.terminal.compress(CompressionMode::Full).unwrap();
    let now = pair.write(&lines(more + 2_000, more + 2_010));

    assert_eq!(now.numbering, began, "the count was never lost");
    assert!(now.first.get() > 2_000, "the oldest rows were dropped, the first now {:?}", now.first);
}

/// Each loses what the count was taken from, so whatever the copy held of the numbering before is gone.
#[test]
fn a_numbering_starts_again_when_its_count_is_lost() {
    let mut pair = Pair::new(SIZE);
    let mut numbering = pair.write(&lines(0, 50)).numbering;
    let mut next = |pair: &mut Pair, what: &str, bytes: &[u8]| {
        let now = pair.write(bytes).numbering;
        assert!(now > numbering, "{what} kept the numbering");
        numbering = now;
    };

    next(&mut pair, "more lines in one write than the history holds", &lines(50, 3 * HISTORY_LINES));
    next(&mut pair, "clearing the history", b"\x1b[3J");
    next(&mut pair, "a reset", b"\x1bc");
    let resized = pair.resize(Size { cols: 30, rows: 5 }).numbering;
    assert!(resized > numbering, "a resize kept the numbering");
}

/// Ghostty failing partway through a write or a resize leaves no count to trust, so the screen has a numbering newer
/// than any before, and nothing asked in one before is sent, until the next write counts again.
#[test]
fn a_count_let_go_after_a_failure_never_names_a_row_again() {
    let mut pair = Pair::new(SIZE);
    let before = pair.write(&lines(0, 50));
    let earlier = pair.copy.history_rows(before.numbering, before.first, 10).unwrap();

    pair.copy.rows.as_mut().unwrap().lost();
    let lost = pair.copy.numbered().unwrap();
    let asked = pair.copy.history_rows(before.numbering, before.first, 10).unwrap();
    let after = pair.copy.history_rows(lost.numbering, lost.first, 10).unwrap();
    let counted = pair.write(&lines(50, 60));

    assert_eq!(earlier.rows.len(), 10);
    assert!(lost.numbering > before.numbering && !lost.shell, "{lost:?}");
    assert!(asked.rows.is_empty() && after.rows.is_empty(), "{asked:?} {after:?}");
    assert!(counted.numbering > lost.numbering, "{counted:?}");
}

/// With no history, the count is on the screen's first row, which a line put above it pushes down. That can't be
/// counted, so it starts again rather than number rows wrong.
#[test]
fn a_line_put_above_the_first_with_no_history_is_never_counted_wrong() {
    let mut pair = Pair::new(SIZE);
    pair.write(b"a\r\nb");

    for put_above in ["\x1b[H\x1bM", "\x1b[H\x1b[L", "\x1b[H\x1b[2L"] {
        pair.write(put_above.as_bytes());
    }
    pair.write(&lines(0, 30));
}

/// The program's screen has no history, and the shell's count waits while it's up, even when a resize meanwhile starts
/// the shell's numbering again.
#[test]
fn a_programs_screen_has_a_numbering_of_its_own() {
    let mut pair = Pair::new(SIZE);
    let shell = pair.write(&lines(0, 30));

    let program = pair.write(b"\x1b[?1049h\x1b[H");
    pair.write(&lines(0, 30));
    let after = pair.write(b"\x1b[?1049l");
    pair.write(b"\x1b[?1049h");
    pair.resize(Size { cols: 30, rows: 5 });
    let resized = pair.write(b"\x1b[?1049l");

    assert!(!program.shell && program.numbering != shell.numbering);
    assert_eq!((program.first, program.top), (RowNumber::new(0), RowNumber::new(0)));
    assert_eq!(after.numbering, shell.numbering, "the shell's count goes on after the program");
    assert!(after.shell && resized.shell);
    assert!(resized.numbering > after.numbering);
}

#[derive(Clone, Debug)]
enum Op {
    Lines { count: usize, width: usize, split: usize },
    InsertLines { row: u16, count: u16 },
    DeleteLines { row: u16, count: u16 },
    ScrollRegion { top: u16, bottom: u16, lines: usize },
    ClearScreen,
    ClearHistory,
    Reset,
    Program { lines: usize },
    Resize { cols: u16 },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        8 => (0..120_usize, 0..60_usize, 0..400_usize).prop_map(|(count, width, split)| Op::Lines { count, width, split }),
        1 => (0..5_u16, 1..4_u16).prop_map(|(row, count)| Op::InsertLines { row, count }),
        1 => (0..5_u16, 1..4_u16).prop_map(|(row, count)| Op::DeleteLines { row, count }),
        1 => (0..3_u16, 2..5_u16, 0..20_usize).prop_map(|(top, bottom, lines)| Op::ScrollRegion { top, bottom, lines }),
        1 => Just(Op::ClearScreen),
        1 => Just(Op::ClearHistory),
        1 => Just(Op::Reset),
        1 => (0..40_usize).prop_map(|lines| Op::Program { lines }),
        1 => (10..40_u16).prop_map(|cols| Op::Resize { cols }),
    ]
}

/// The bytes of `op`, as one or two writes.
fn bytes(op: &Op, n: &mut usize) -> Vec<Vec<u8>> {
    let mut line = |width: usize| {
        *n += 1;
        format!("{n:06}{}\r\n", "x".repeat(width))
    };
    match op {
        Op::Lines { count, width, split } => {
            let all: String = (0..*count).map(|_| line(*width)).collect();
            let at = (*split).min(all.len());
            vec![all.as_bytes()[..at].to_vec(), all.as_bytes()[at..].to_vec()]
        }
        Op::InsertLines { row, count } => vec![format!("\x1b[{};1H\x1b[{count}L", row + 1).into_bytes()],
        Op::DeleteLines { row, count } => vec![format!("\x1b[{};1H\x1b[{count}M", row + 1).into_bytes()],
        Op::ScrollRegion { top, bottom, lines } => {
            let body: String = (0..*lines).map(|_| line(0)).collect();
            vec![format!("\x1b[{};{}r\x1b[{};1H{body}\x1b[r", top + 1, bottom + 1, bottom + 1).into_bytes()]
        }
        Op::ClearScreen => vec![b"\x1b[H\x1b[2J".to_vec()],
        Op::ClearHistory => vec![b"\x1b[3J".to_vec()],
        Op::Reset => vec![b"\x1bc".to_vec()],
        Op::Program { lines } => {
            let body: String = (0..*lines).map(|_| line(3)).collect();
            vec![format!("\x1b[?1049h\x1b[H{body}").into_bytes(), b"\x1b[?1049l".to_vec()]
        }
        Op::Resize { .. } => Vec::new(),
    }
}

proptest! {
    #![proptest_config(Config { cases: 48, rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// Whatever a program writes, within a numbering the count moves only as the rows that scroll into history do,
    /// and a row of history read once holds the same under its number for as long as the terminal keeps it.
    #[test]
    fn a_row_number_never_names_another_row(ops in proptest::collection::vec(op(), 1..40)) {
        let mut pair = Pair::nearly_full(SIZE);
        let mut seen: HashMap<(Numbering, RowNumber), TerminalRow> = HashMap::new();
        let mut n = 0;
        for op in &ops {
            if let Op::Resize { cols } = op {
                pair.resize(Size { cols: *cols, rows: SIZE.rows });
            }
            for write in bytes(op, &mut n) {
                pair.write(&write);
            }
            let now = pair.check();
            if !now.shell {
                continue;
            }
            for number in [now.first.get(), u64::midpoint(now.first.get(), now.top.get()), now.top.get().saturating_sub(1)] {
                let number = RowNumber::new(number);
                if number < now.first || number >= now.top {
                    continue;
                }
                let read = row(&pair.copy.terminal, Place::Kept(number.get() - now.first.get())).unwrap();
                let held = seen.entry((now.numbering, number)).or_insert_with(|| read.clone());
                prop_assert_eq!(&*held, &read, "row {:?} changed under its number", number);
            }
        }
    }
}
