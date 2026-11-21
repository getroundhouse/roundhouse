use protocol::terminal::{Key, KeyPress, Mods, TerminalSize, Typed};

use crate::Mirror;

fn mirror_of(written: &[u8]) -> Mirror {
    let mut mirror = Mirror::new(TerminalSize { cols: 20, rows: 4 }).unwrap();
    mirror.write(written);
    mirror
}

fn press(key: Key, mods: Mods, text: Option<&str>) -> KeyPress {
    let consumed = Mods { shift: mods.shift && text.is_some(), ..Mods::default() };
    KeyPress { key, mods, text: text.map(str::to_owned), consumed }
}

const NONE: Mods = Mods { shift: false, ctrl: false, alt: false, super_key: false };
const CTRL: Mods = Mods { ctrl: true, ..NONE };
const ALT: Mods = Mods { alt: true, ..NONE };
const SHIFT: Mods = Mods { shift: true, ..NONE };

#[test]
fn a_key_is_read_as_a_terminal_sends_it() {
    let mut mirror = mirror_of(b"");
    let mut sent = |key, mods, text| mirror.key(&press(key, mods, text)).unwrap();

    assert_eq!(sent(Key::Char('a'), NONE, Some("a")), b"a");
    assert_eq!(sent(Key::Char('a'), SHIFT, Some("A")), b"A");
    assert_eq!(sent(Key::Char('c'), CTRL, None), b"\x03");
    // Option types a character on a Mac, as it does in other apps, and Alt goes before the key elsewhere.
    if cfg!(target_os = "macos") {
        assert_eq!(sent(Key::Char('x'), ALT, Some("≈")), "≈".as_bytes());
    } else {
        assert_eq!(sent(Key::Char('x'), ALT, Some("x")), b"\x1bx");
    }
    assert_eq!(sent(Key::Enter, NONE, None), b"\r");
    assert_eq!(sent(Key::Backspace, NONE, None), b"\x7f");
    assert_eq!(sent(Key::Tab, SHIFT, None), b"\x1b[Z");
    assert_eq!(sent(Key::Up, NONE, None), b"\x1b[A");
    assert_eq!(sent(Key::F(1), NONE, None), b"\x1bOP");
    assert_eq!(sent(Key::Char('é'), NONE, Some("é")), "é".as_bytes(), "a character a US keyboard has no key for");
}

/// A program that asks for the cursor keys' application mode, as a full-screen one does, reads them another way.
#[test]
fn a_key_is_read_by_the_modes_the_program_set() {
    let mut mirror = mirror_of(b"\x1b[?1h");

    assert_eq!(mirror.key(&press(Key::Up, NONE, None)).unwrap(), b"\x1bOA");
}

/// Unmarked, each line ends as Return does, so a shell runs it.
#[test]
fn a_paste_loses_its_escapes_and_has_its_marks_if_the_program_asked() {
    let mut mirror = mirror_of(b"");

    let plain = mirror.paste("a\nb\x1bc").unwrap();
    mirror.write(b"\x1b[?2004h");
    let bracketed = mirror.paste("a\nb").unwrap();

    assert_eq!(plain, b"a\rb c");
    assert_eq!(bracketed, b"\x1b[200~a\nb\x1b[201~");
}

/// A full-screen program's screen has no history, so it's told of the wheel as arrow keys, to scroll itself.
#[test]
fn the_wheel_over_a_full_screen_program_sends_arrow_keys() {
    let mut mirror = mirror_of(b"\x1b[?1049h");

    assert_eq!(mirror.typed(&Typed::Wheel { lines: -2 }).unwrap(), b"\x1b[A\x1b[A");
    assert_eq!(mirror.typed(&Typed::Wheel { lines: 1 }).unwrap(), b"\x1b[B");
}

/// Until the mouse is sent to programs, since one that asked for it, such as tmux, takes arrow keys for keys typed.
#[test]
fn the_wheel_over_a_program_that_asked_for_the_mouse_sends_nothing() {
    let mut mirror = mirror_of(b"\x1b[?1049h\x1b[?1000h");

    assert_eq!(mirror.typed(&Typed::Wheel { lines: -1 }).unwrap(), b"");
}

/// The worker's copy is never scrolled, since each client scrolls its own, so where it says the cursor is stays put. A
/// turn is held to a screen's worth, since each line is a key, and a key a newer client knows is sent as its text.
#[test]
fn what_the_worker_is_sent_never_scrolls_its_copy() {
    let mut shell = mirror_of(&b"line\r\n".repeat(10));
    let cursor = shell.shown().unwrap().cursor;
    let wheel = shell.typed(&Typed::Wheel { lines: -3 }).unwrap();
    let mut program = mirror_of(b"\x1b[?1049h");
    let unknown = Typed::Key(KeyPress { key: Key::Unknown, mods: NONE, text: Some("x".into()), consumed: NONE });

    assert_eq!((wheel, shell.shown().unwrap().cursor), (Vec::new(), cursor));
    assert!(cursor.is_some());
    assert_eq!(program.typed(&Typed::Wheel { lines: i32::MAX }).unwrap(), b"\x1b[B".repeat(4));
    assert_eq!(program.typed(&unknown).unwrap(), b"x");
}
