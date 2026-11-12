use libghostty_vt::mode::Mode;
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::Index;
use proptest::test_runner::{Config, RngSeed};

use super::*;

/// Everything a client's terminal should agree with the worker's on, as far as what it draws and how it reads keys.
#[derive(Debug, PartialEq, Eq)]
struct Shows {
    screen: Screen,
    cursor: (u16, u16),
    cursor_visible: bool,
    cursor_pending_wrap: bool,
    modes: Vec<bool>,
    title: String,
    pwd: String,
    history_rows: usize,
    text: String,
    styled: Vec<u8>,
}

fn shows(mirror: &Mirror) -> Shows {
    let t = &mirror.terminal;
    let mut plain = Formatter::new(t).unwrap();
    plain.set_format(Format::Plain).unwrap();
    let modes = [Mode::DECCKM, Mode::BRACKETED_PASTE, Mode::CURSOR_VISIBLE, Mode::ALT_SCREEN_SAVE, Mode::SGR_MOUSE];
    Shows {
        screen: t.active_screen().unwrap(),
        cursor: (t.cursor_x().unwrap(), t.cursor_y().unwrap()),
        cursor_visible: t.is_cursor_visible().unwrap(),
        cursor_pending_wrap: t.is_cursor_pending_wrap().unwrap(),
        modes: modes.iter().map(|&m| t.mode(m).unwrap()).collect(),
        title: String::from_utf8_lossy(t.title().unwrap()).into_owned(),
        pwd: String::from_utf8_lossy(t.pwd().unwrap()).into_owned(),
        history_rows: t.scrollback_rows().unwrap(),
        text: String::from_utf8_lossy(&plain.format_alloc(None).unwrap()).into_owned(),
        styled: screen(t).unwrap(),
    }
}

const SIZE: Size = Size { cols: 80, rows: 24 };

/// A client's terminal, made from what the worker's snapshot says.
fn attached(worker: &Mirror, size: Size) -> Mirror {
    let mut client = Mirror::new(size).unwrap();
    client.write(&worker.snapshot().unwrap());
    client
}

fn shell_with_history() -> Mirror {
    let mut worker = Mirror::new(SIZE).unwrap();
    worker.write(b"\x1b]7;file://host/work\x07\x1b]2;cargo test\x07");
    for line in 0..100 {
        worker.write(format!("\x1b[3{}mline {line}\x1b[0m caf\u{e9} \u{4e2d}\u{6587}\r\n", line % 8).as_bytes());
    }
    worker.write(b"\x1b[?1h\x1b[?2004h$ ");
    worker
}

#[test]
fn a_new_terminal_shows_the_shell_and_its_history_again() {
    let worker = shell_with_history();

    let client = attached(&worker, SIZE);

    assert_eq!(shows(&client), shows(&worker));
    assert!(shows(&client).history_rows > 0);
}

#[test]
fn quitting_a_full_screen_program_brings_back_what_was_under_it() {
    let mut worker = shell_with_history();
    worker.write(b"\x1b[?1049h\x1b[2J\x1b[H\x1b[44;97m vim \x1b[0m\x1b[5;10Hhello\x1b[?25l\x1b[3;3H");

    let mut client = attached(&worker, SIZE);

    assert_eq!(shows(&client), shows(&worker));
    for terminal in [&mut worker, &mut client] {
        terminal.write(b"\x1b[?25h\x1b[?1049l");
    }
    assert_eq!(shows(&client), shows(&worker));
    assert!(shows(&client).history_rows > 0);
}

#[test]
fn what_comes_after_a_sequence_cut_off_partway_lands_as_it_does_on_the_worker() {
    let mut worker = shell_with_history();
    worker.write(b"\x1b[3");

    let mut client = attached(&worker, SIZE);

    for terminal in [&mut worker, &mut client] {
        terminal.write(b"1mred\x1b[0m");
    }
    assert_eq!(shows(&client), shows(&worker));
}

/// Pieces of what programs write: words, line breaks, colours, moves, clears, a full-screen program starting and
/// quitting, modes, a title and a working folder.
fn piece() -> impl Strategy<Value = Vec<u8>> {
    let words = prop_oneof![
        "[a-z ]{1,24}".prop_map(String::into_bytes),
        Just("caf\u{e9}".into()),
        Just("\u{4e2d}\u{6587}".into()),
        Just("\u{1f980}".into()),
    ];
    let controls =
        prop_oneof![Just(b"\r\n".to_vec()), Just(b"\n".to_vec()), Just(b"\r".to_vec()), Just(b"\t".to_vec())];
    let style = prop_oneof![
        (0..110u8).prop_map(|n| format!("\x1b[{n}m").into_bytes()),
        (0..=255u8).prop_map(|n| format!("\x1b[38;5;{n}m").into_bytes()),
        (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(r, g, b)| format!("\x1b[48;2;{r};{g};{b}m").into_bytes()),
    ];
    let placing = prop_oneof![
        (1..30u8, 1..90u8).prop_map(|(row, col)| format!("\x1b[{row};{col}H").into_bytes()),
        Just(b"\x1b[2J".to_vec()),
        Just(b"\x1b[J".to_vec()),
        Just(b"\x1b[K".to_vec()),
    ];
    let modes = prop_oneof![
        Just(b"\x1b[?1049h".to_vec()),
        Just(b"\x1b[?1049l".to_vec()),
        Just(b"\x1b[?1h".to_vec()),
        Just(b"\x1b[?1l".to_vec()),
        Just(b"\x1b[?2004h".to_vec()),
        Just(b"\x1b[?25l".to_vec()),
        Just(b"\x1b[?25h".to_vec()),
        "[a-z ]{0,12}".prop_map(|title| format!("\x1b]2;{title}\x07").into_bytes()),
        "[a-z]{1,8}".prop_map(|folder| format!("\x1b]7;file://host/{folder}\x07").into_bytes()),
    ];
    prop_oneof![4 => words, 3 => controls, 2 => style, 2 => placing, 1 => modes]
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    /// The reason the type exists: a client attaching at any byte of the output, partway through a sequence or a
    /// character included, gets the snapshot and then the rest, and shows what the worker does, on the program's
    /// screen and, once the program quits, on the shell's under it.
    #[test]
    fn attaching_at_any_point_ends_with_what_the_worker_has(pieces in vec(piece(), 0..80), at in any::<Index>()) {
        let size = Size { cols: 20, rows: 6 };
        let output = pieces.concat();
        let (before, after) = output.split_at(at.index(output.len() + 1));
        let mut worker = Mirror::new(size).unwrap();
        worker.write(before);

        let mut client = attached(&worker, size);
        for terminal in [&mut worker, &mut client] {
            terminal.write(after);
        }

        prop_assert_eq!(shows(&client), shows(&worker));
        // Only while a program's screen is up, since on the shell's, leaving it would go back to a cursor saved by
        // the last program to start, which a snapshot doesn't carry, and no program asks for.
        if shows(&worker).screen == Screen::Alternate {
            for terminal in [&mut worker, &mut client] {
                terminal.write(TO_SHELL_SCREEN);
            }
            prop_assert_eq!(shows(&client), shows(&worker));
        }
    }
}
