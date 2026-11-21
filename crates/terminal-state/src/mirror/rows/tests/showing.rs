//! What the worker's copy shows for a picture: the screen's rows by number, the cursor, and the rest of its state.

use std::fmt::Write as _;

use protocol::terminal::{CellColor, CursorShape, PaletteColor, RowJson, RowNumber, ScreenState, Wheel};

use crate::Mirror;
use crate::mirror::rows::reading::{Place, row};

use super::pair::{SIZE, lines};

fn written(bytes: &str) -> Mirror {
    let mut mirror = Mirror::new(SIZE).unwrap();
    mirror.write(bytes.as_bytes());
    mirror
}

fn text(row: &RowJson) -> String {
    row.row().spans.iter().map(|span| span.text.as_str()).collect()
}

#[test]
fn the_screen_is_its_rows_from_the_top_and_history_is_by_number() {
    let mut mirror = Mirror::new(SIZE).unwrap();
    mirror.write(&lines(0, 100));
    mirror.write(b"$ ");

    let shown = mirror.shown().unwrap();
    let newest = mirror.history_row(RowNumber::new(shown.top.get() - 1)).map(|row| text(&row));
    let oldest = mirror.history_row(shown.first).map(|row| text(&row));

    let rows: Vec<String> = shown.rows.iter().map(text).collect();
    assert_eq!(rows, ["line 96", "line 97", "line 98", "line 99", "$ "]);
    assert_eq!(shown.top, mirror.numbered().unwrap().top);
    assert_eq!((newest.as_deref(), oldest.as_deref()), (Some("line 95"), Some("line 0")));
    assert_eq!(mirror.history_row(shown.top), None, "the screen's rows aren't history");
    let cursor = shown.cursor.unwrap();
    assert_eq!((cursor.col, cursor.row, cursor.shape), (2, 4, CursorShape::Block));
}

/// On either cell of a wide character, the cursor is on its first, as Ghostty draws it.
#[test]
fn a_cursor_is_on_a_wide_characters_first_cell_shaped_as_asked_and_absent_while_hidden() {
    let mut mirror = written("日\x1b[2G\x1b[6 q");
    let on_wide = mirror.shown().unwrap().cursor.unwrap();
    mirror.write(b"\x1b[4 q");
    let underline = mirror.shown().unwrap().cursor.unwrap();
    mirror.write(b"\x1b[?25l");
    let hidden = mirror.shown().unwrap().cursor;

    assert_eq!((on_wide.col, on_wide.shape), (0, CursorShape::Bar));
    assert_eq!(underline.shape, CursorShape::Underline);
    assert_eq!(hidden, None);
}

#[test]
fn state_says_what_the_program_set_that_isnt_in_its_rows() {
    let mut mirror = written("");
    let before = mirror.shown().unwrap().state;
    mirror.write(b"\x1b]2;build\x07\x1b]4;1;rgb:ff/00/00\x07\x1b]10;rgb:12/34/56\x07\x1b[?5h");
    let set = mirror.shown().unwrap().state;

    assert_eq!(before, ScreenState::default());
    assert_eq!(set.title, "build");
    assert_eq!(set.palette, [PaletteColor { index: 1, color: CellColor::Rgb { r: 0xff, g: 0, b: 0 } }]);
    assert_eq!(set.foreground, Some(CellColor::Rgb { r: 0x12, g: 0x34, b: 0x56 }));
    assert!(set.reverse && !set.program && set.wheel == Wheel::Scrolls, "{set:?}");
}

/// As a turn of the wheel does in each, so a client can tell at once whether it scrolls its own view.
#[test]
fn a_programs_screen_says_what_the_wheel_does_there() {
    let mut mirror = written("$ less\r\n\x1b[?1049h\x1b[?1007l\x1b[2;1Ha page");
    let wheel = |mirror: &mut Mirror| mirror.shown().unwrap().state.wheel;

    let scrolls = wheel(&mut mirror);
    mirror.write(b"\x1b[?1007h");
    let arrows = wheel(&mut mirror);
    mirror.write(b"\x1b[?1000h");
    let program = wheel(&mut mirror);
    let shown = mirror.shown().unwrap();

    assert_eq!((scrolls, arrows, program), (Wheel::Scrolls, Wheel::Arrows, Wheel::Program));
    assert!(shown.state.program && shown.top == shown.first, "a program's screen has no history: {shown:?}");
    assert_eq!(shown.rows.iter().take(2).map(text).collect::<Vec<_>>(), ["", "a page"]);
}

#[test]
fn a_program_that_asks_for_its_output_shown_all_at_once_says_so_until_it_has_finished() {
    let mut mirror = written("\x1b[?2026h");
    let asked = mirror.synchronized();
    mirror.write(b"\x1b[?2026l");

    assert!(asked && !mirror.synchronized());
}

/// A row read from the screen's top is the same as it read from the oldest row kept, as history is, so a sender can
/// tell a row that left the screen is as its receiver has it.
#[test]
fn a_row_on_the_screen_reads_the_same_from_either_end() {
    let mut mirror = Mirror::new(SIZE).unwrap();
    for line in 0..40 {
        let mut cells = String::new();
        for col in 0..20 {
            let _ = write!(cells, "\x1b[38;5;{};48;5;{col}m日", (line + col) % 256);
        }
        mirror.write(format!("{cells}\x1b[0m e\u{301}\r\n").as_bytes());
    }
    let now = mirror.numbered().unwrap();
    let above = now.top.get() - now.first.get();

    for y in 0..SIZE.rows {
        let screen = row(&mirror.terminal, Place::Screen(y)).unwrap();
        let kept = row(&mirror.terminal, Place::Kept(above + u64::from(y))).unwrap();
        assert_eq!(screen, kept, "row {y}");
    }
    let styled = (0..SIZE.rows).any(|y| !row(&mirror.terminal, Place::Screen(y)).unwrap().styles.is_empty());
    assert!(styled, "no row had the colours written");
}
