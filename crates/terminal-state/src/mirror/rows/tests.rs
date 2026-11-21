use std::collections::HashMap;
use std::fmt::Write as _;

use libghostty_vt::terminal::CompressionMode;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use protocol::terminal::{CellAttributes, CellColor, CellStyle, Numbering, RowNumber, Span, TerminalRow};
use protocol::worker::MOST_ROWS;

use super::counting::Numbered;
use super::reading::row;
use crate::mirror::HISTORY_LINES;
use crate::{Mirror, Size};

const SIZE: Size = Size { cols: 20, rows: 5 };

fn written(bytes: &str) -> Mirror {
    let mut mirror = Mirror::new(SIZE).unwrap();
    mirror.write(bytes.as_bytes());
    mirror
}

fn text(row: &TerminalRow) -> String {
    row.spans.iter().map(|span| span.text.as_str()).collect()
}

fn span(text: &str) -> Span {
    Span { text: text.into(), ..Span::default() }
}

/// Colours as the program gave them, each style once, and a span for each run in one.
#[test]
fn a_row_is_its_runs_of_cells_in_one_style() {
    let mirror = written("\x1b[1;31mhi\x1b[0m \x1b[38;2;1;2;3mx\x1b[0m");

    let read = row(&mirror.terminal, 0).unwrap();

    let bold_red = CellStyle {
        foreground: Some(CellColor::Palette(1)),
        attributes: CellAttributes::default().with(CellAttributes::BOLD, true),
        ..CellStyle::default()
    };
    let rgb = CellStyle { foreground: Some(CellColor::Rgb { r: 1, g: 2, b: 3 }), ..CellStyle::default() };
    assert_eq!(read.styles, [bold_red, rgb]);
    let styled = |text: &str, style: u16| Span { style: Some(style), ..span(text) };
    assert_eq!(read.spans, [styled("hi", 0), span(" "), styled("x", 1)]);
}

/// Erasing in a colour leaves cells with no text, whose colour is kept in the cell rather than in a style.
#[test]
fn cells_erased_in_a_colour_are_blank_in_it() {
    let mirror = written("ab\x1b[44m\x1b[K");

    let read = row(&mirror.terminal, 0).unwrap();

    assert_eq!(read.styles, [CellStyle { background: Some(CellColor::Palette(4)), ..CellStyle::default() }]);
    assert_eq!(read.spans, [span("ab"), Span { blank: SIZE.cols - 2, style: Some(0), ..Span::default() }]);
}

/// A wide character takes two cells, a cluster is one cell's, and a gap starts the next span at its column.
#[test]
fn wide_characters_clusters_and_gaps_say_where_their_cells_are() {
    let long = format!("a{}", "\u{301}".repeat(20));
    let mirror = written(&format!("\u{4e2d}e\u{301}x\x1b[12Gy{long}"));

    let read = row(&mirror.terminal, 0).unwrap();

    assert_eq!(
        read.spans,
        [
            Span { wide: true, ..span("\u{4e2d}") },
            Span { cluster: true, ..span("e\u{301}") },
            span("x"),
            Span { col: Some(11), ..span("y") },
            Span { cluster: true, ..span(&long) },
        ]
    );
}

#[test]
fn a_row_says_whether_its_line_goes_on_in_the_next() {
    let mirror = written(&"x".repeat(usize::from(SIZE.cols) + 5));

    let (first, second) = (row(&mirror.terminal, 0).unwrap(), row(&mirror.terminal, 1).unwrap());

    assert!(first.wraps);
    assert!(!second.wraps);
    assert_eq!(text(&second), "xxxxx");
}

/// A copy that never drops a row, against which another's count is checked: the rows that have scrolled into its
/// history are the screen's first row's number in a numbering that started with none.
fn reference(size: Size) -> Mirror {
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
struct Pair {
    copy: Mirror,
    reference: Mirror,
    began: (Numbering, u64, u64),
}

impl Pair {
    /// With the history a terminal keeps. Ghostty drops rows a page at a time, so a smaller cap than a page of rows,
    /// about 1,300 at 20 columns, would lose the count on every write, and test nothing.
    fn new(size: Size) -> Self {
        let mut pair = Self {
            copy: Mirror::new(size).unwrap(),
            reference: reference(size),
            began: (Numbering::new(u64::MAX), 0, 0),
        };
        pair.check();
        pair
    }

    /// With its history nearly full, so what's written next drops the oldest rows.
    fn nearly_full(size: Size) -> Self {
        let mut pair = Self::new(size);
        pair.write(&lines(0, HISTORY_LINES - 50));
        pair
    }

    fn write(&mut self, bytes: &[u8]) -> Numbered {
        self.copy.write(bytes);
        self.reference.write(bytes);
        self.check()
    }

    fn resize(&mut self, size: Size) -> Numbered {
        self.copy.resize(size).unwrap();
        self.reference.resize(size).unwrap();
        self.check()
    }

    /// Fails if the copy's numbering is the one it was, and its count has moved by other than the reference's.
    fn check(&mut self) -> Numbered {
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

fn lines(from: usize, to: usize) -> Vec<u8> {
    let mut written = String::new();
    for n in from..to {
        let _ = write!(written, "line {n}\r\n");
    }
    written.into_bytes()
}

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
                let read = row(&pair.copy.terminal, number.get() - now.first.get()).unwrap();
                let held = seen.entry((now.numbering, number)).or_insert_with(|| read.clone());
                prop_assert_eq!(&*held, &read, "row {:?} changed under its number", number);
            }
        }
    }
}

/// By number, in the numbering asked for, and none in another, which the reply says.
#[test]
fn history_is_read_by_row_number() {
    let mut mirror = Mirror::new(SIZE).unwrap();
    mirror.write(&lines(0, 100));
    let now = mirror.numbered().unwrap();

    let asked = mirror.history_rows(now.numbering, RowNumber::new(10), 3).unwrap();
    let elsewhere = mirror.history_rows(now.numbering.next(), RowNumber::new(10), 3).unwrap();
    let past_the_history = mirror.history_rows(now.numbering, now.top.plus(u64::MAX >> 1), 3).unwrap();

    assert_eq!(asked.rows.iter().map(text).collect::<Vec<_>>(), ["line 10", "line 11", "line 12"]);
    assert_eq!(asked.from, RowNumber::new(10));
    assert_eq!((elsewhere.numbering, elsewhere.rows.len()), (now.numbering, 0));
    assert!(past_the_history.rows.is_empty(), "the screen's rows aren't history");
}

#[test]
fn a_programs_screen_has_no_history_to_ask_for() {
    let mut mirror = Mirror::new(SIZE).unwrap();
    mirror.write(&lines(0, 100));
    let shell = mirror.numbered().unwrap().numbering;
    mirror.write(b"\x1b[?1049h");

    let asked = mirror.history_rows(shell, RowNumber::new(10), 3).unwrap();

    assert!(asked.rows.is_empty());
    assert_ne!(asked.numbering, shell);
}

/// So one ask keeps the terminal's thread from what its program writes for only a few milliseconds.
#[test]
fn an_ask_takes_at_most_its_most_rows_or_about_a_frames_worth() {
    let mut plain = Mirror::new(SIZE).unwrap();
    plain.write(&lines(0, 1_000));
    let mut colourful = Mirror::new(Size { cols: 200, rows: 5 }).unwrap();
    for line in 0..300 {
        let mut cells = String::new();
        for col in 0..200 {
            let _ = write!(cells, "\x1b[38;5;{}mx", (line + col) % 256);
        }
        colourful.write(format!("{cells}\x1b[0m\r\n").as_bytes());
    }

    let first = |mirror: &mut Mirror| {
        let now = mirror.numbered().unwrap();
        mirror.history_rows(now.numbering, now.first, 1_000).unwrap().rows
    };
    let (plain, colourful) = (first(&mut plain), first(&mut colourful));

    assert_eq!(plain.len(), usize::try_from(MOST_ROWS).unwrap());
    let bytes = |rows: &[TerminalRow]| rows.iter().map(|row| serde_json::to_vec(row).unwrap().len()).sum::<usize>();
    assert!(colourful.len() < plain.len() && bytes(&colourful) < 80 * 1024, "{} rows", colourful.len());
}
