//! What a row says of its cells, and what's read of history by number.

use std::fmt::Write as _;

use protocol::terminal::{CellAttributes, CellColor, CellStyle, MOST_ROWS, RowNumber, Span, TerminalRow, TerminalSize};

use crate::Mirror;
use crate::mirror::rows::reading::{Place, row};

use super::pair::{SIZE, lines};

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

    let read = row(&mirror.terminal, Place::Kept(0)).unwrap();

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

    let read = row(&mirror.terminal, Place::Kept(0)).unwrap();

    assert_eq!(read.styles, [CellStyle { background: Some(CellColor::Palette(4)), ..CellStyle::default() }]);
    assert_eq!(read.spans, [span("ab"), Span { blank: SIZE.cols - 2, style: Some(0), ..Span::default() }]);
}

/// A wide character takes two cells, a cluster is one cell's, and a gap starts the next span at its column.
#[test]
fn wide_characters_clusters_and_gaps_say_where_their_cells_are() {
    let long = format!("a{}", "\u{301}".repeat(20));
    let mirror = written(&format!("\u{4e2d}e\u{301}x\x1b[12Gy{long}"));

    let read = row(&mirror.terminal, Place::Kept(0)).unwrap();

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

    let (first, second) =
        (row(&mirror.terminal, Place::Kept(0)).unwrap(), row(&mirror.terminal, Place::Kept(1)).unwrap());

    assert!(first.wraps);
    assert!(!second.wraps);
    assert_eq!(text(&second), "xxxxx");
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
    let mut colourful = Mirror::new(TerminalSize { cols: 200, rows: 5 }).unwrap();
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
