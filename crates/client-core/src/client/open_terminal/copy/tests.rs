use protocol::terminal::{TerminalBytes, TerminalExit, TerminalScreen};
use terminal_state::keys::{Key, Mods};

use super::*;

/// Not white on black, which Ghostty draws in until it's told other colours.
const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
const BLUE: Rgb = Rgb { r: 0, g: 0, b: 255 };
const SMALL: Size = Size { cols: 20, rows: 4 };
const BIG: Size = Size { cols: 30, rows: 6 };

fn screen(size: Size, bytes: &[u8]) -> TerminalUpdate {
    TerminalUpdate::Screen(TerminalScreen { size: terminal_size(size), bytes: bytes.into() })
}

fn output(bytes: &[u8]) -> TerminalUpdate {
    TerminalUpdate::Output { output: TerminalBytes::from(bytes) }
}

fn text(frame: &Frame, row: usize) -> String {
    frame.rows[row].text()
}

/// The key that types `typed`, with nothing held.
pub(in crate::client::open_terminal) fn key(typed: char) -> KeyPress {
    KeyPress { key: Key::Char(typed), mods: Mods::default(), text: Some(typed.to_string()), consumed: Mods::default() }
}

#[test]
fn a_copy_shows_the_screen_then_what_comes_after_it() {
    let mut copy = TerminalCopy::new();
    copy.set_colors(RED, BLUE).unwrap();
    assert_eq!(copy.frame().unwrap(), None, "nothing to show before the screen");

    copy.take_in(screen(SMALL, b"$ ")).unwrap();
    copy.take_in(output(b"ls")).unwrap();
    let frame = copy.frame().unwrap().unwrap();

    assert_eq!(text(&frame, 0), "$ ls");
    assert_eq!((frame.foreground, frame.background), (RED, BLUE), "in the view's colours");
    assert_eq!(frame.rows[0].glyphs[0].color, RED, "a character that sets no colour");
    assert_eq!(copy.frame().unwrap(), None, "nothing has changed since");
}

/// A screen starts again from nothing, so what came before it is gone.
#[test]
fn a_new_screen_replaces_what_the_copy_had() {
    let mut copy = TerminalCopy::new();
    copy.take_in(screen(SMALL, b"old")).unwrap();

    copy.take_in(screen(BIG, b"new")).unwrap();

    let frame = copy.frame().unwrap().unwrap();
    assert_eq!((text(&frame, 0), frame.size), ("new".to_owned(), BIG));
}

/// So a view that attaches, or attaches again, takes the terminal back to its own size.
#[test]
fn a_screen_of_another_size_asks_for_the_one_the_view_has_room_for() {
    let mut copy = TerminalCopy::new();
    assert_eq!(copy.fit(BIG).unwrap(), ToHub::default(), "nothing to resize yet");

    let other = copy.take_in(screen(SMALL, b"")).unwrap();
    let same = copy.take_in(screen(BIG, b"")).unwrap();

    assert_eq!(other.resize, Some(terminal_size(BIG)));
    assert_eq!(same.resize, None);
}

/// Another client showing it gave it that size, so it isn't fought over.
#[test]
fn a_new_size_from_elsewhere_is_kept() {
    let mut copy = TerminalCopy::new();
    copy.fit(SMALL).unwrap();
    copy.take_in(screen(SMALL, b"")).unwrap();

    let asks = copy.take_in(TerminalUpdate::Resized { size: terminal_size(BIG) }).unwrap();

    assert_eq!(asks.resize, None);
    assert_eq!(copy.frame().unwrap().unwrap().size, BIG);
    assert_eq!(
        copy.fit(Size { cols: 25, rows: 5 }).unwrap().resize,
        Some(TerminalSize { cols: 25, rows: 5 }),
        "until the view's room changes"
    );
    assert_eq!(copy.fit(Size { cols: 25, rows: 5 }).unwrap().resize, None, "once");
}

/// The room going back to what it was before the terminal has taken the size asked for in between asks for it
/// again, rather than leaving the terminal at the size between.
#[test]
fn room_that_goes_back_before_the_terminal_has_changed_asks_for_its_size_again() {
    let mut copy = TerminalCopy::new();
    copy.fit(SMALL).unwrap();
    copy.take_in(screen(SMALL, b"")).unwrap();

    let bigger = copy.fit(BIG).unwrap();
    let back = copy.fit(SMALL).unwrap();
    copy.take_in(TerminalUpdate::Resized { size: terminal_size(BIG) }).unwrap();
    let after = copy.take_in(TerminalUpdate::Resized { size: terminal_size(SMALL) }).unwrap();

    assert_eq!((bigger.resize, back.resize), (Some(terminal_size(BIG)), Some(terminal_size(SMALL))));
    assert_eq!(after.resize, None);
    assert_eq!(copy.frame().unwrap().unwrap().size, SMALL);
    assert_eq!(copy.fit(SMALL).unwrap().resize, None, "the room hasn't changed");
}

#[test]
fn a_key_is_sent_as_the_program_reads_it_and_goes_back_to_the_screen() {
    let mut copy = TerminalCopy::new();
    assert_eq!(copy.key(&key('a')).unwrap().input, b"", "nothing before the screen, whose modes it's read by");
    let history: Vec<u8> = (0..10).flat_map(|line| format!("{line}\r\n").into_bytes()).collect();
    copy.take_in(screen(SMALL, &history)).unwrap();
    let on_screen = text(&copy.frame().unwrap().unwrap(), 0);
    copy.wheel(-3).unwrap();
    assert_ne!(text(&copy.frame().unwrap().unwrap(), 0), on_screen, "scrolled back through the history");

    let typed = copy.key(&key('a')).unwrap();

    assert_eq!(typed.input, b"a");
    assert_eq!(text(&copy.frame().unwrap().unwrap(), 0), on_screen);
}

#[test]
fn an_end_changes_nothing_shown() {
    let mut copy = TerminalCopy::new();
    copy.take_in(screen(SMALL, b"$ ")).unwrap();
    copy.frame().unwrap();

    copy.take_in(TerminalUpdate::Exited(TerminalExit { code: Some(0) })).unwrap();

    assert_eq!(copy.frame().unwrap(), None);
}

/// As for a view made once the screen has come.
#[test]
fn colours_told_after_the_screen_are_drawn_in() {
    let mut copy = TerminalCopy::new();
    copy.take_in(screen(SMALL, b"$ ")).unwrap();
    copy.frame().unwrap();

    copy.set_colors(RED, BLUE).unwrap();

    let frame = copy.frame().unwrap().expect("drawn again in them");
    assert_eq!((frame.foreground, frame.background), (RED, BLUE));
}
