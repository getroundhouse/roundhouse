//! That a window draws what Ghostty drew, cell for cell, colour for colour, from pictures.

use std::sync::Arc;

use protocol::terminal::{PictureSending, TerminalSize};

use super::described::described;
use super::feeding::{pictured, watched, worker};

const SIZE: TerminalSize = TerminalSize { cols: 24, rows: 6 };

/// What Ghostty drew of each output here, in the view's colours from `feeding`, at [`SIZE`], as [`described`] writes
/// it, each under its output.
const DRAWN: &str = include_str!("painted.txt");

/// What Ghostty drew under `heading` in [`DRAWN`], up to the blank line after it, a line at a time, however a checkout
/// ends its lines.
fn drawn(heading: &str) -> String {
    let mut lines = DRAWN.lines().skip_while(|line| *line != heading);
    assert!(lines.next().is_some(), "nothing recorded for {heading}");
    lines.take_while(|line| !line.is_empty()).fold(String::new(), |drawn, line| drawn + line + "\n")
}

/// What a window draws of `output` from a worker's pictures of it, against what Ghostty drew.
fn both(output: &str) {
    let mut worker = worker(SIZE, output.as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());

    assert_eq!(described(&window.frame().unwrap()), drawn(&format!("{output:?}")), "{output:?}");
}

#[test]
fn text_and_the_cursor_are_drawn_as_ghostty_drew_them() {
    both("$ ls\r\nCargo.toml  src\r\n$ ");
    both("one\r\ntwo\r\n\x1b[?25l");
    both("\x1b[5 qbar");
    both("\x1b[3 qunder\x1b[2;3H");
}

#[test]
fn colours_from_the_palette_and_by_value_are_drawn_as_ghostty_drew_them() {
    both("\x1b[31mred\x1b[0m \x1b[1;94mbright\x1b[0m \x1b[38;5;208morange\x1b[0m");
    both("\x1b[38;2;10;200;30mrgb\x1b[48;2;1;2;3m on\x1b[0m \x1b[48;5;236m grey \x1b[0m");
    both("\x1b[42m\x1b[Kcleared in green\x1b[0m\r\n\x1b[44m  \x1b[0m");
}

#[test]
fn the_styles_of_text_are_drawn_as_ghostty_drew_them() {
    both("\x1b[1mbold\x1b[0m \x1b[3mitalic\x1b[0m \x1b[2mfaint\x1b[0m \x1b[4mline\x1b[0m \x1b[9mthrough\x1b[0m");
    both("\x1b[7minverse\x1b[0m \x1b[31;7mred inverse\x1b[0m \x1b[8mhidden\x1b[0m");
    both("\x1b[4m   \x1b[0m blank but underlined");
}

#[test]
fn wide_characters_and_clusters_are_drawn_as_ghostty_drew_them() {
    both("日本語 text");
    both("e\u{301} and 👍🏽 too");
    both("\x1b[41m漢\x1b[0m字\x1b[1;2H");
}

#[test]
fn colours_the_program_changed_are_drawn_as_ghostty_drew_them() {
    both("\x1b]4;1;rgb:12/34/56\x07\x1b[31mmine\x1b[0m");
    both("\x1b]10;rgb:aa/bb/cc\x07\x1b]11;rgb:01/02/03\x07plain");
    both("\x1b]12;rgb:ff/00/ff\x07cursor");
    both("\x1b[?5hreversed\x1b[31m red");
}

/// Before the view says its colours, a cell that sets none is drawn in Ghostty's own.
#[test]
fn before_the_view_gives_its_colours_ghosttys_are_used() {
    let output = "plain \x1b[7minverse";
    let mut worker = worker(SIZE, output.as_bytes());
    let mut window = super::super::Window::new();
    for frame in pictured(&mut worker, &mut PictureSending::default()) {
        window.take(frame).unwrap();
    }

    assert_eq!(described(&window.frame().unwrap()), drawn(&format!("own colours {output:?}")));
}

/// A row that hasn't changed is the same `Arc` in the next frame, so a view can tell it needn't lay it out again.
#[test]
fn a_row_that_hasnt_changed_is_the_one_drawn_before() {
    let mut worker = worker(SIZE, b"one\r\ntwo\r\n");
    let mut sending = PictureSending::default();
    let mut window = watched(&mut worker, &mut sending);
    let before = window.frame().unwrap();

    worker.write(b"three");
    for frame in pictured(&mut worker, &mut sending) {
        assert!(window.take(frame).unwrap().is_empty());
    }
    let after = window.frame().unwrap();

    assert!(Arc::ptr_eq(&before.rows[0], &after.rows[0]), "\"one\" is as it was");
    assert!(!Arc::ptr_eq(&before.rows[2], &after.rows[2]), "\"three\" is new");
}
