//! What's pressed in a terminal, as a client sends it its hub and the hub its worker, and what a reader makes of a
//! form a newer peer writes.

use protocol::client::terminals::TerminalTyped;
use protocol::client::{ClientFrame, ClientRequest};
use protocol::terminal::{Key, KeyPress, Mods, Typed};
use protocol::worker::{self, WorkerRequest};

use crate::keeps_its_form;
use crate::worker_calls::asked;

fn press(key: Key, mods: Mods, text: Option<&str>, consumed: Mods) -> Typed {
    Typed::Key(KeyPress { key, mods, text: text.map(str::to_owned), consumed })
}

const NONE: Mods = Mods { shift: false, ctrl: false, alt: false, super_key: false };
const SHIFT: Mods = Mods { shift: true, ..NONE };

#[test]
fn what_a_client_and_a_hub_send_of_whats_pressed_keeps_its_form() {
    let typed = Typed::Paste { text: "ls\n".into() };
    let client = TerminalTyped { id: "t".into(), typed: typed.clone() };
    keeps_its_form(
        &ClientFrame { id: 7, request: ClientRequest::TerminalTyped(client) },
        r#"{"id":7,"request":{"method":"terminal_typed","id":"t","typed":{"kind":"paste","text":"ls\n"}}}"#,
    );
    asked(
        WorkerRequest::TerminalTyped(worker::TerminalTyped { terminal: "t".into(), typed }),
        r#"{"method":"terminal_typed","terminal":"t","typed":{"kind":"paste","text":"ls\n"}}"#,
    );
}

/// Every kind of thing pressed, with a key that types a character, a named key, a function key and every modifier,
/// each modifier left out while it isn't held.
#[test]
fn everything_pressed_keeps_its_form() {
    let every = Mods { shift: true, ctrl: true, alt: true, super_key: true };
    let forms = [
        (press(Key::Char('a'), NONE, Some("a"), NONE), r#"{"kind":"key","key":"a","text":"a"}"#),
        (
            press(Key::Char('a'), SHIFT, Some("A"), SHIFT),
            r#"{"kind":"key","key":"a","mods":{"shift":true},"text":"A","consumed":{"shift":true}}"#,
        ),
        (
            press(Key::Char(';'), every, None, NONE),
            r#"{"kind":"key","key":";","mods":{"shift":true,"ctrl":true,"alt":true,"super":true}}"#,
        ),
        (press(Key::PageDown, NONE, None, NONE), r#"{"kind":"key","key":"page_down"}"#),
        (press(Key::F(12), NONE, None, NONE), r#"{"kind":"key","key":"f12"}"#),
        (Typed::Paste { text: "two\nlines".into() }, r#"{"kind":"paste","text":"two\nlines"}"#),
        (Typed::Wheel { lines: -3 }, r#"{"kind":"wheel","lines":-3}"#),
        (Typed::Text { text: "日本".into() }, r#"{"kind":"text","text":"日本"}"#),
    ];
    for (typed, form) in forms {
        keeps_its_form(&typed, form);
    }
    for key in [
        Key::Enter,
        Key::Tab,
        Key::Backspace,
        Key::Escape,
        Key::Space,
        Key::Up,
        Key::Down,
        Key::Left,
        Key::Right,
        Key::Home,
        Key::End,
        Key::PageUp,
        Key::Insert,
        Key::Delete,
    ] {
        let written = serde_json::to_string(&key).unwrap();
        assert_eq!(serde_json::from_str::<Key>(&written).unwrap(), key, "{written}");
        assert!(written.len() > 3, "{key:?} is written as a name, not a character: {written}");
    }
}

/// What a newer peer may press: a kind, a key and a modifier this build doesn't know, which the program is sent
/// nothing for, or the text alone.
#[test]
fn what_a_newer_peer_presses_still_reads() {
    let read = |line: &str| serde_json::from_str::<Typed>(line).unwrap();

    assert_eq!(read(r#"{"kind":"pinch","scale":2}"#), Typed::Unknown);
    assert_eq!(read(r#"{"kind":"key","key":"media_play","text":"x"}"#), press(Key::Unknown, NONE, Some("x"), NONE));
    assert_eq!(read(r#"{"kind":"key","key":"f13"}"#), press(Key::Unknown, NONE, None, NONE));
    assert_eq!(
        read(r#"{"kind":"key","key":"a","mods":{"shift":true,"hyper":true}}"#),
        press(Key::Char('a'), SHIFT, None, NONE)
    );
}

/// What's typed may be a password, so it never shows in a log.
#[test]
fn whats_typed_is_left_out_of_debug() {
    let shown = format!(
        "{:?} {:?} {:?}",
        press(Key::Char('s'), NONE, Some("s3cret"), NONE),
        Typed::Paste { text: "s3cret".into() },
        Typed::Text { text: "s3cret".into() },
    );

    assert!(!shown.contains("s3cret") && !shown.contains("'s'"), "{shown}");
}
