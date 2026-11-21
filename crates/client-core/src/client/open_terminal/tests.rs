// Against a stand-in hub on a Unix socket, which sends what a hub sends of a terminal.
#![cfg(unix)]

use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::following::*;
use crate::client::Client;
use crate::stand_in_hub::{Does, listen};
use protocol::client::terminals::{TerminalFit, TerminalTyped};
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, HubFrame, ReplyOutcome};
use protocol::terminal::{
    Key, KeyPress, Mods, PictureSending, TerminalExit, TerminalPicture, TerminalSize, TerminalUpdate, Typed,
};
use terminal_state::drawn::{Frame, Rgb};
use terminal_state::{Mirror, Size};

const SIZE: TerminalSize = TerminalSize { cols: 80, rows: 24 };
const WAIT: Duration = Duration::from_secs(10);
const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
const BLUE: Rgb = Rgb { r: 0, g: 0, b: 255 };

/// The key that types `typed`, with nothing held.
fn key(typed: char) -> KeyPress {
    KeyPress { key: Key::Char(typed), mods: Mods::default(), text: Some(typed.to_string()), consumed: Mods::default() }
}

fn typed(typed: char) -> ClientRequest {
    ClientRequest::TerminalTyped(TerminalTyped { id: "t".into(), typed: Typed::Key(key(typed)) })
}

/// The pictures a worker sends of a terminal that's written each of `writes` in turn, one after each.
fn pictures(writes: &[&[u8]]) -> Vec<TerminalPicture> {
    let mut worker = Mirror::new(Size { cols: SIZE.cols, rows: SIZE.rows }).unwrap();
    let mut sending = PictureSending::default();
    let mut pictures = Vec::new();
    for written in writes {
        worker.write(written);
        let mut next = sending.next(worker.shown().unwrap(), |number| worker.history_row(number));
        while let Some(picture) = next {
            pictures.push(picture);
            next = sending.rest();
        }
    }
    pictures
}

/// What a hub sends of a terminal it's watched: pictures, and maybe the shell's end.
struct Watched {
    pictures: Vec<TerminalPicture>,
    exited: bool,
}

/// A hub that answers each watch of a terminal by sending what `watched_for` gives for how many watches there have
/// been, or that there's no such terminal for `None`, and says each other request it's sent, and each watch. It goes
/// away once it's sent what's typed, if `hangs_up_on_typing`.
fn start_hub(
    socket: &Path,
    hangs_up_on_typing: bool,
    watched_for: impl Fn(usize) -> Option<Watched> + Send + 'static,
) -> flume::Receiver<ClientRequest> {
    let (asked, told) = flume::unbounded();
    let mut watched_count = 0;
    let ok = || Box::new(ReplyOutcome::Response(ClientResponse::Ok));
    listen(socket, move |request| {
        let ClientRequest::TerminalWatch(watch) = &request else {
            let hangs_up = hangs_up_on_typing && matches!(request, ClientRequest::TerminalTyped(_));
            let _ = asked.send(request);
            return if hangs_up { Does::HangsUp } else { Does::Answers(ok()) };
        };
        watched_count += 1;
        let Some(watched) = watched_for(watched_count) else {
            let gone = ApiError::new(ErrorCode::NotFound, "no such terminal");
            return Does::Answers(Box::new(ReplyOutcome::Error(gone)));
        };
        let subscription_id = watch.subscription_id;
        let mut sent: Vec<HubFrame> = watched
            .pictures
            .into_iter()
            .map(|picture| HubFrame::TerminalPicture { subscription_id, picture })
            .collect();
        if watched.exited {
            let update = TerminalUpdate::Exited(TerminalExit { code: Some(0) });
            sent.push(HubFrame::Terminal { subscription_id, update });
        }
        let _ = asked.send(request);
        Does::AnswersThen(ok(), sent)
    });
    told
}

/// A terminal that wrote each of `writes`, with a picture after each, and hasn't ended.
fn showing(writes: &[&[u8]]) -> Watched {
    Watched { pictures: pictures(writes), exited: false }
}

fn text(frame: &Frame) -> Vec<String> {
    frame.rows.iter().map(|row| row.text()).collect()
}

/// The first frame a view of `open` takes for which `shows` is true, taking each as it comes.
fn frame_when(open: &OpenTerminal, what: &str, shows: impl Fn(&Frame) -> bool) -> Arc<Frame> {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(frame) = open.take_frame()
            && shows(&frame)
        {
            return frame;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(open.changed().recv_timeout(left).is_ok(), "{what} never showed");
    }
}

/// Until its shell has ended.
fn until_ended(open: &OpenTerminal) {
    let deadline = Instant::now() + WAIT;
    while !open.ended() {
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(open.changed().recv_timeout(left).is_ok(), "it never ended");
    }
}

/// A view takes what the terminal shows, then what comes after, in its own colours, and what's pressed goes to the
/// hub, for the worker to turn into what the program reads. The shell's end ends it.
#[test]
fn an_open_terminal_keeps_what_it_shows_for_a_view_to_draw() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let asked =
        start_hub(&socket, false, |_| Some(Watched { pictures: pictures(&[b"$ ", b"ls\r\n", b"a.rs"]), exited: true }));
    let client = Client::connect(&socket, "test").unwrap();

    let open = OpenTerminal::start(&client, &"t".into());
    open.set_colors(RED, BLUE);
    let frame = frame_when(&open, "what came after the first picture", |frame| text(frame)[1] == "a.rs");
    open.key(key('x'));
    let room = Size { cols: 100, rows: 30 };
    open.fit(room);

    assert_eq!(text(&frame)[0], "$ ls");
    assert_eq!((frame.foreground, frame.background), (RED, BLUE));
    assert!(matches!(asked.recv_timeout(WAIT).unwrap(), ClientRequest::TerminalWatch(_)));
    assert_eq!(asked.recv_timeout(WAIT).unwrap(), typed('x'));
    let ClientRequest::TerminalFit(TerminalFit { size, .. }) = asked.recv_timeout(WAIT).unwrap() else {
        panic!("the view's room wasn't asked for")
    };
    assert_eq!((size.cols, size.rows), (room.cols, room.rows));
    until_ended(&open);
}

/// As after the hub restarts: it starts again from a picture of what the terminal shows, on the new connection, and
/// what's typed
/// goes there. What's typed as the new connection comes, before the thread has heard the last go, which here it never
/// does, goes to neither, as typing over a connection that's going doesn't.
#[test]
fn a_terminal_carries_on_on_a_new_connection() {
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("first.sock"), dir.path().join("second.sock"));
    let first_asked = start_hub(&first, false, |_| Some(showing(&[b"before"])));
    let second_asked = start_hub(&second, false, |_| Some(showing(&[b"after"])));
    let open = OpenTerminal::start(&Client::connect(&first, "test").unwrap(), &"t".into());
    frame_when(&open, "the first screen", |frame| text(frame)[0] == "before");

    let next = Client::connect(&second, "test").unwrap();
    open.follow.quietly(|held| {
        held.given.push(Given::Key(key('w')));
        open.reconnect(&next);
    });
    frame_when(&open, "the second screen", |frame| text(frame)[0] == "after");
    open.key(key('x'));

    assert!(matches!(second_asked.recv_timeout(WAIT).unwrap(), ClientRequest::TerminalWatch(_)));
    assert_eq!(second_asked.recv_timeout(WAIT).unwrap(), typed('x'), "not what was typed as it came");
    assert!(first_asked.try_iter().all(|asked| !matches!(asked, ClientRequest::TerminalTyped(_))));
}

/// The view scrolls through the rows it holds with no connection, but what's typed then is dropped, rather than sent
/// once there's a connection again, as typing over one that's gone is.
#[test]
fn without_a_connection_the_held_history_scrolls_and_what_is_typed_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("first.sock"), dir.path().join("second.sock"));
    let lines = |lines: std::ops::Range<usize>| {
        lines.fold(String::new(), |mut text, line| {
            let _ = write!(text, "line {line}\r\n");
            text
        })
    };
    let (early, late) = (lines(0..20), lines(20..40));
    // The first picture, then the next after 17 rows have scrolled off, which the client holds.
    let _first_asked = start_hub(&first, true, move |_| Some(showing(&[early.as_bytes(), late.as_bytes()])));
    let second_asked = start_hub(&second, false, |_| Some(showing(&[b"after"])));
    let open = OpenTerminal::start(&Client::connect(&first, "test").unwrap(), &"t".into());
    frame_when(&open, "the screen", |frame| text(frame)[0] == "line 17");

    open.key(key('a'));
    waiting::thread::until(WAIT, "the hub never went", || !open.connected());
    open.wheel(-3);
    frame_when(&open, "the history", |frame| text(frame)[0] == "line 14");
    open.key(key('b'));
    // Taken in with nothing to send it on, so it goes back to the screen.
    frame_when(&open, "the screen again", |frame| text(frame)[0] == "line 17");
    // Typed as the next connection comes, so the thread hears of both at once.
    let next = Client::connect(&second, "test").unwrap();
    open.follow.quietly(|held| {
        held.given.push(Given::Key(key('d')));
        open.reconnect(&next);
    });
    frame_when(&open, "the second screen", |frame| text(frame)[0] == "after");
    open.key(key('c'));

    assert!(matches!(second_asked.recv_timeout(WAIT).unwrap(), ClientRequest::TerminalWatch(_)));
    assert_eq!(second_asked.recv_timeout(WAIT).unwrap(), typed('c'), "not what was typed while there was no hub");
}

/// As one does that ended while there was no connection to hear it, which a view has to hear of to say so.
#[test]
fn a_terminal_whose_record_has_gone_has_ended() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let _asked = start_hub(&socket, false, |_| None);
    let client = Client::connect(&socket, "test").unwrap();

    let open = OpenTerminal::start(&client, &"t".into());

    until_ended(&open);
    assert!(open.take_errors().is_empty(), "it isn't a failure to hear of");
}

/// Through the hub, as the app does it: rows a turn of the wheel scrolls back to are fetched and drawn, on the thread
/// that waits for replies, and what's selected is copied as the worker copies it.
#[test]
fn rows_scrolled_back_to_are_fetched_and_what_is_selected_is_copied() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub");
    let mut world = crate::fake_hub::World::default();
    let repo = world.repo("aspen");
    let workspace = world.workspace(&repo, "elm");
    let lines = (0..100).fold(String::new(), |mut text, line| {
        let _ = write!(text, "line {line}\r\n");
        text
    });
    let id = world.terminal(&workspace, SIZE, lines.as_bytes());
    crate::fake_hub::serve(&socket, world);
    let client = Client::connect(&socket, "test").unwrap();
    let open = OpenTerminal::start(&client, &id);
    frame_when(&open, "the screen", |frame| text(frame)[0] == "line 77");

    open.wheel(-60);
    frame_when(&open, "the history", |frame| text(frame)[0] == "line 17");
    open.press(terminal_state::Pointer { col: 0.1, row: 1.5 }, terminal_state::SelectBy::Cells);
    open.drag(terminal_state::Pointer { col: 6.9, row: 2.5 });
    open.release();
    let copied = open.selected_text().recv_timeout(WAIT);

    assert_eq!(copied.as_deref(), Ok("line 18\nline 19"));
}
