use super::*;
use crate::Size;

fn mirror_of(written: &[u8]) -> Mirror {
    let mut mirror = Mirror::new(Size { cols: 20, rows: 4 }).unwrap();
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

    assert_eq!(mirror.wheel(-2).unwrap(), b"\x1b[A\x1b[A");
    assert_eq!(mirror.wheel(1).unwrap(), b"\x1b[B");
}

/// Until the mouse is sent to programs, since one that asked for it, such as tmux, takes arrow keys for keys typed.
#[test]
fn the_wheel_over_a_program_that_asked_for_the_mouse_sends_nothing() {
    let mut mirror = mirror_of(b"\x1b[?1049h\x1b[?1000h");

    assert_eq!(mirror.wheel(-1).unwrap(), b"");
}
