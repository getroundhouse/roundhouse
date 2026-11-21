//! That a window draws what the client's copy of Ghostty drew, cell for cell, colour for colour, from pictures.

use protocol::terminal::PictureSending;
use terminal_state::{Mirror, Size};

use super::feeding::{BACKGROUND, FOREGROUND, pictured, watched, worker};

const SIZE: Size = Size { cols: 24, rows: 6 };

/// What a client's own copy of Ghostty draws of `output`, against what a window draws from a worker's pictures of it.
fn both(output: &str) {
    let mut ghostty = Mirror::new(SIZE).unwrap();
    ghostty.set_colors(FOREGROUND, BACKGROUND).unwrap();
    ghostty.write(output.as_bytes());
    let drawn = ghostty.frame().unwrap();

    let mut worker = worker(SIZE, output.as_bytes());
    let mut window = watched(&mut worker, &mut PictureSending::default());
    let from_pictures = window.frame().unwrap();

    for (at, (ghosttys, windows)) in drawn.rows.iter().zip(&from_pictures.rows).enumerate() {
        assert_eq!(windows, ghosttys, "row {at} of {output:?}");
    }
    assert_eq!(*from_pictures, drawn, "{output:?}");
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
    let mut ghostty = Mirror::new(SIZE).unwrap();
    ghostty.write(output.as_bytes());
    let drawn = ghostty.frame().unwrap();

    let mut worker = worker(SIZE, output.as_bytes());
    let mut window = super::super::Window::new();
    for frame in pictured(&mut worker, &mut PictureSending::default()) {
        window.take(frame).unwrap();
    }

    assert_eq!(*window.frame().unwrap(), drawn);
}
