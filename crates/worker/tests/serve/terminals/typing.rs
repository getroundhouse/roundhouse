//! What's pressed in a terminal, which its worker turns into what the program reads by the modes it set.

use protocol::terminal::{Key, KeyPress, Mods, Typed};
use protocol::worker::{TerminalTyped, WorkerOutcome, WorkerRequest, WorkerResponse};

use super::hub::Hub;

async fn press(hub: &mut Hub, typed: Typed) {
    let pressed = hub.call(WorkerRequest::TerminalTyped(TerminalTyped { terminal: "t".into(), typed })).await;
    assert_eq!(pressed, WorkerOutcome::Response(WorkerResponse::Ok));
}

fn key(key: Key) -> Typed {
    Typed::Key(KeyPress { key, mods: Mods::default(), text: None, consumed: Mods::default() })
}

/// A program that reads `count` bytes as they're typed and writes each in hex, once it has set what `modes` set, and
/// said so, and says once the terminal is back as the shell reads lines, for what's typed next.
fn reading(modes: &str, count: usize) -> String {
    // The quotes keep the shell's echo of the line from reading as its having said so.
    format!("stty raw -echo; printf '{modes}rea''dy'; head -c {count} | od -An -tx1 | tr -s ' '; stty sane; echo do''ne")
}

/// The up arrow is read as the program asked in what it wrote just before, which only the worker's copy of the
/// terminal knows: as application mode's after it sets that mode.
#[tokio::test]
async fn a_key_is_read_by_the_mode_the_program_set_just_before() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;

    hub.type_in("t", &reading(r"\033[?1h", 3)).await;
    hub.until_output("ready").await;
    press(&mut hub, key(Key::Up)).await;

    hub.until_output("1b 4f 41").await;
    hub.finish().await;
}

/// A turn of the wheel on the shell's screen is nothing to the program, since each client scrolls its own history,
/// and on a program's screen that asked for it, the arrow keys. What's pasted and text as it is reach it as they are.
#[tokio::test]
async fn the_wheel_is_read_as_arrows_only_on_a_programs_screen_that_asked() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;

    hub.type_in("t", &reading("", 2)).await;
    hub.until_output("ready").await;
    press(&mut hub, Typed::Wheel { lines: -3 }).await;
    press(&mut hub, Typed::Text { text: "é".into() }).await;
    hub.until_output("c3 a9").await;
    hub.until_output("done").await;

    hub.type_in("t", &reading(r"\033[?1049h\033[?1007h", 7)).await;
    hub.until_output("ready").await;
    press(&mut hub, Typed::Wheel { lines: 2 }).await;
    press(&mut hub, Typed::Paste { text: "x".into() }).await;
    hub.until_output("1b 5b 42 1b 5b 42 78").await;
    hub.finish().await;
}
