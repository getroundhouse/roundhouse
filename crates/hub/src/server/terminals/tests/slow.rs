//! A client that's slow to take what it's sent of a terminal, as one on a slow link is. On a paused clock, where a
//! sleep lasts until everything else is waiting, so what the hub does meanwhile has all been done.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use protocol::client::terminals::{TerminalAttach, TerminalClose, TerminalInput};
use protocol::client::{ClientRequest, HubFrame};
use protocol::terminal::{TerminalExit, TerminalSize, TerminalUpdate, UpdateIndex};

use super::super::backlog::{BEHIND_BYTES, BEHIND_UPDATES, JOINED_OUTPUT_BYTES};
use super::stand_in::{Client, Connected, joined, open_on, output, screen, shown};
use crate::hub::Hub;
use crate::worker_link::WorkerLink;

/// Until everything else is waiting. Only a moment on the clock, so the hub's own timers, of seconds, don't pass.
#[expect(
    clippy::disallowed_methods,
    reason = "on tokio's paused clock, which moves on once everything else is waiting"
)]
pub(super) async fn settled() {
    tokio::time::sleep(Duration::from_millis(1)).await;
}

/// Line `n`, of `width` bytes, its number padded with zeros, so lines out of order or missing show.
fn line(n: u64, width: usize) -> String {
    format!("{n:0width$}", width = width - 1) + "\n"
}

/// What a client hasn't taken yet waits in its follower, so it has one frame of it queued at a time, and what comes
/// meanwhile is joined into frames of up to a worker's read.
#[tokio::test(start_paused = true)]
async fn a_slow_client_has_one_frame_queued_and_what_comes_meanwhile_joined() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link) = open_on(dir.path(), |_| Ok(shown(0))).await;
    let mut client = Client::attach(&hub);
    settled().await;
    let lines: Vec<String> = (0..200).map(|n| line(n, 1024)).collect();

    for (at, text) in (0..).zip(&lines) {
        hub.terminal_update(&link, "t".into(), UpdateIndex::new(at), output(text));
        settled().await;
    }

    assert_eq!(client.queued(), 1, "only the screen");
    assert_eq!(client.next().await, TerminalUpdate::Screen(screen()));
    let mut frames = Vec::new();
    for _ in 0..4 {
        frames.push(client.next().await);
    }
    let sizes: Vec<usize> = frames.iter().map(frame_bytes).collect();
    assert_eq!(sizes, [JOINED_OUTPUT_BYTES, JOINED_OUTPUT_BYTES, JOINED_OUTPUT_BYTES, 8 * 1024]);
    assert_eq!(joined(frames), (lines.concat(), Vec::new()));
}

fn frame_bytes(update: &TerminalUpdate) -> usize {
    let TerminalUpdate::Output { output } = update else { panic!("sent {update:?}") };
    output.len()
}

/// A hub whose terminal's client hasn't read the screen it was sent, and has been sent too much more since, which is
/// `count` updates that `flood` makes, and how many times the worker has been asked what the terminal shows. It says
/// the screens after the first take in the flood.
async fn flooded(dir: &Path, flood: fn(u64) -> TerminalUpdate, count: u64) -> Flooded {
    let shows = Arc::new(AtomicUsize::new(0));
    let counting = shows.clone();
    let (hub, link) = open_on(dir, move |asked| {
        counting.fetch_add(1, Ordering::SeqCst);
        Ok(shown(if asked == 0 { 0 } else { count }))
    })
    .await;
    let client = Client::attach(&hub);
    settled().await;
    for at in 0..count {
        hub.terminal_update(&link, "t".into(), UpdateIndex::new(at), flood(at));
        settled().await;
    }
    Flooded { hub, link, client, shows }
}

struct Flooded {
    hub: Arc<Hub>,
    link: Arc<WorkerLink>,
    client: Client,
    shows: Arc<AtomicUsize>,
}

/// Updates enough to put a client too far behind.
struct Flood {
    what: &'static str,
    update: fn(u64) -> TerminalUpdate,
    count: usize,
}

fn new_size(at: u64) -> TerminalUpdate {
    TerminalUpdate::Resized { size: TerminalSize { cols: 80 + u16::try_from(at % 2).unwrap(), rows: 24 } }
}

/// Once more waits for it than a screen would take, it's sent what the terminal shows, as one that falls behind
/// otherwise is: more output, or more updates, such as another client's new sizes. The worker is asked for that only
/// once there's room to send it, and only once however much more comes meanwhile, so a slow client doesn't keep its
/// terminal's worker busy with screens nobody is sent.
#[tokio::test(start_paused = true)]
async fn a_client_too_far_behind_is_sent_one_screen_once_theres_room() {
    let floods = [
        Flood {
            what: "output",
            update: |_| output(&line(0, JOINED_OUTPUT_BYTES)),
            count: 3 * (BEHIND_BYTES / JOINED_OUTPUT_BYTES),
        },
        Flood { what: "new sizes", update: new_size, count: 3 * BEHIND_UPDATES },
    ];
    for Flood { what, update: flood, count } in floods {
        let dir = tempfile::tempdir().unwrap();
        let after = u64::try_from(count).unwrap();
        let Flooded { hub, link, mut client, shows } = flooded(dir.path(), flood, after).await;
        let asked_while_full = shows.load(Ordering::SeqCst);
        let first = client.next().await;
        settled().await;
        hub.terminal_update(&link, "t".into(), UpdateIndex::new(after), output("after"));

        assert_eq!(asked_while_full, 1, "{what}: only for the first screen");
        assert_eq!(first, TerminalUpdate::Screen(screen()));
        assert_eq!(client.next().await, TerminalUpdate::Screen(screen()), "{what}: with nothing before it");
        assert_eq!(client.next().await, output("after"));
        assert_eq!(shows.load(Ordering::SeqCst), 2);
    }
}

/// While it's too far behind, the follower holds nothing, and waits to send a screen, which there's none of once the
/// terminal has gone, so it's sent the end instead. What the shell wrote last, and how it ended, are lost, as for a
/// client that falls behind otherwise.
#[tokio::test(start_paused = true)]
async fn a_terminal_that_ends_while_its_client_is_too_far_behind_ends_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let count = u64::try_from(2 * (BEHIND_BYTES / JOINED_OUTPUT_BYTES)).unwrap();
    let Flooded { hub, mut client, .. } = flooded(dir.path(), |_| output(&line(0, JOINED_OUTPUT_BYTES)), count).await;

    hub.terminal_close(TerminalClose { id: "t".into() }).await.unwrap();

    assert_eq!(client.next().await, TerminalUpdate::Screen(screen()));
    assert_eq!(client.rest().await, [TerminalUpdate::Exited(TerminalExit { code: None })]);
}

/// Over a connection to a client that has stopped reading, a reply waits behind only what's being written and one
/// frame of each terminal, rather than every frame of a flood of output.
#[tokio::test(start_paused = true)]
async fn a_reply_to_a_slow_client_waits_behind_little_of_a_terminals_output() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link) = open_on(dir.path(), |_| Ok(shown(0))).await;
    let mut connected = Connected::to(&hub, 16 * 1024).await;
    connected.send(1, ClientRequest::TerminalAttach(TerminalAttach { id: "t".into(), subscription_id: 7 })).await;
    settled().await;

    for at in 0..2000 {
        hub.terminal_update(&link, "t".into(), UpdateIndex::new(at), output(&line(at, 1024)));
        if at % 100 == 0 {
            settled().await;
        }
    }
    let typed = TerminalInput { id: "t".into(), input: b"x".as_slice().into() };
    connected.send(2, ClientRequest::TerminalInput(typed)).await;
    settled().await;

    let mut before = 0;
    loop {
        match connected.next().await {
            HubFrame::Reply { id: 2, .. } => break,
            HubFrame::Terminal { update: TerminalUpdate::Output { .. }, .. } => before += 1,
            // The reply to attaching, and the screen.
            HubFrame::Reply { .. }
            | HubFrame::Terminal { .. }
            | HubFrame::TerminalPicture { .. }
            | HubFrame::Event { .. }
            | HubFrame::Tail { .. }
            | HubFrame::Delta { .. }
            | HubFrame::Notice { .. }
            | HubFrame::Github { .. } => {}
        }
    }
    assert!(before <= 3, "{before} frames of output came first");
}
