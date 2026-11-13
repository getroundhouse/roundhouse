// Against a stand-in hub on a Unix socket, which sends what a hub sends of a terminal.
#![cfg(unix)]

use std::path::Path;
use std::time::{Duration, Instant};

use super::copy::tests::key;
use super::*;
use crate::stand_in_hub::{Does, listen};
use protocol::client::terminals::{TerminalInput, TerminalResize};
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, HubFrame, ReplyOutcome};
use protocol::terminal::{TerminalBytes, TerminalScreen, TerminalSize};

const SIZE: TerminalSize = TerminalSize { cols: 80, rows: 24 };
const WAIT: Duration = Duration::from_secs(10);
const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
const BLUE: Rgb = Rgb { r: 0, g: 0, b: 255 };

fn screen(bytes: &[u8]) -> TerminalUpdate {
    TerminalUpdate::Screen(TerminalScreen { size: SIZE, bytes: bytes.into() })
}

fn output(bytes: &[u8]) -> TerminalUpdate {
    TerminalUpdate::Output { output: TerminalBytes::from(bytes) }
}

fn typed(bytes: &[u8]) -> ClientRequest {
    ClientRequest::TerminalInput(TerminalInput { id: "t".into(), input: bytes.into() })
}

/// A hub that answers each attach to a terminal by sending what `updates_for` gives for how many attaches there have
/// been, or that there's no such terminal for `None`, and says each other request it's sent, and each attach. It goes
/// away once it's sent what's typed, if `hangs_up_on_typing`.
fn start_hub(
    socket: &Path,
    hangs_up_on_typing: bool,
    updates_for: impl Fn(usize) -> Option<Vec<TerminalUpdate>> + Send + 'static,
) -> flume::Receiver<ClientRequest> {
    let (asked, told) = flume::unbounded();
    let mut attaches = 0;
    let ok = || Box::new(ReplyOutcome::Response(ClientResponse::Ok));
    listen(socket, move |request| {
        let ClientRequest::TerminalAttach(attach) = &request else {
            let hangs_up = hangs_up_on_typing && matches!(request, ClientRequest::TerminalInput(_));
            let _ = asked.send(request);
            return if hangs_up { Does::HangsUp } else { Does::Answers(ok()) };
        };
        attaches += 1;
        let Some(updates) = updates_for(attaches) else {
            let gone = ApiError::new(ErrorCode::NotFound, "no such terminal");
            return Does::Answers(Box::new(ReplyOutcome::Error(gone)));
        };
        let subscription_id = attach.subscription_id;
        let sent = updates.into_iter().map(|update| HubFrame::Terminal { subscription_id, update }).collect();
        let _ = asked.send(request);
        Does::AnswersThen(ok(), sent)
    });
    told
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

/// A view takes what the terminal shows, then what comes after, in its own colours, and what's typed goes to the hub as
/// the program reads it. The shell's end ends it.
#[test]
fn an_open_terminal_keeps_what_it_shows_for_a_view_to_draw() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let asked = start_hub(&socket, false, |_| {
        let exited = TerminalUpdate::Exited(protocol::terminal::TerminalExit { code: Some(0) });
        Some(vec![screen(b"$ "), output(b"ls\r\n"), TerminalUpdate::Unknown, output(b"a.rs"), exited])
    });
    let client = Client::connect(&socket, "test").unwrap();

    let open = OpenTerminal::start(&client, &"t".into());
    open.set_colors(RED, BLUE);
    let frame = frame_when(&open, "what came after the screen", |frame| text(frame)[1] == "a.rs");
    open.key(key('x'));
    let room = Size { cols: 100, rows: 30 };
    open.fit(room);

    assert_eq!(text(&frame)[0], "$ ls", "without the update it can't read");
    assert_eq!((frame.foreground, frame.background), (RED, BLUE));
    assert!(matches!(asked.recv_timeout(WAIT).unwrap(), ClientRequest::TerminalAttach(_)));
    assert_eq!(asked.recv_timeout(WAIT).unwrap(), typed(b"x"));
    let resized = TerminalResize { id: "t".into(), size: TerminalSize { cols: room.cols, rows: room.rows } };
    assert_eq!(asked.recv_timeout(WAIT).unwrap(), ClientRequest::TerminalResize(resized));
    until_ended(&open);
}

/// As after the hub restarts: it starts again from what the terminal shows, on the new connection, and what's typed
/// goes there. What's typed as the new connection comes, before the thread has heard the last go, which here it never
/// does, goes to neither, as typing over a connection that's going doesn't.
#[test]
fn a_terminal_carries_on_on_a_new_connection() {
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("first.sock"), dir.path().join("second.sock"));
    let first_asked = start_hub(&first, false, |_| Some(vec![screen(b"before")]));
    let second_asked = start_hub(&second, false, |_| Some(vec![screen(b"after")]));
    let open = OpenTerminal::start(&Client::connect(&first, "test").unwrap(), &"t".into());
    frame_when(&open, "the first screen", |frame| text(frame)[0] == "before");

    let next = Client::connect(&second, "test").unwrap();
    open.follow.quietly(|held| {
        held.given.push(Given::Key(key('w')));
        open.reconnect(&next);
    });
    frame_when(&open, "the second screen", |frame| text(frame)[0] == "after");
    open.key(key('x'));

    assert!(matches!(second_asked.recv_timeout(WAIT).unwrap(), ClientRequest::TerminalAttach(_)));
    assert_eq!(second_asked.recv_timeout(WAIT).unwrap(), typed(b"x"), "not what was typed as it came");
    assert!(first_asked.try_iter().all(|asked| !matches!(asked, ClientRequest::TerminalInput(_))));
}

/// The copy scrolls through its history with no connection, as it's the client's, but what's typed then is dropped,
/// rather than sent once there's a connection again, as typing over one that's gone is.
#[test]
fn without_a_connection_the_history_scrolls_and_what_is_typed_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (dir.path().join("first.sock"), dir.path().join("second.sock"));
    let history: Vec<u8> = (0..40).flat_map(|line| format!("line {line}\r\n").into_bytes()).collect();
    let _first_asked = start_hub(&first, true, move |_| Some(vec![screen(&history)]));
    let second_asked = start_hub(&second, false, |_| Some(vec![screen(b"after")]));
    let open = OpenTerminal::start(&Client::connect(&first, "test").unwrap(), &"t".into());
    frame_when(&open, "the screen", |frame| text(frame)[0] == "line 17");

    open.key(key('a'));
    let deadline = Instant::now() + WAIT;
    while open.connected() {
        assert!(Instant::now() < deadline, "the hub never went");
        std::thread::sleep(Duration::from_millis(10));
    }
    open.wheel(-3);
    frame_when(&open, "the history", |frame| text(frame)[0] == "line 14");
    open.key(key('b'));
    // Taken in with nothing to send it on, so it scrolls back to the screen.
    frame_when(&open, "the screen again", |frame| text(frame)[0] == "line 17");
    // Typed as the next connection comes, so the thread hears of both at once.
    let next = Client::connect(&second, "test").unwrap();
    open.follow.quietly(|held| {
        held.given.push(Given::Key(key('d')));
        open.reconnect(&next);
    });
    frame_when(&open, "the second screen", |frame| text(frame)[0] == "after");
    open.key(key('c'));

    assert!(matches!(second_asked.recv_timeout(WAIT).unwrap(), ClientRequest::TerminalAttach(_)));
    assert_eq!(second_asked.recv_timeout(WAIT).unwrap(), typed(b"c"), "not what was typed while there was no hub");
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
