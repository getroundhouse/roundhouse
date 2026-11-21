//! Following a terminal for a client: the screen, then what comes after it, until the terminal ends, and what's typed
//! and new sizes passed on in order.

use std::fmt::Write as _;
use std::sync::{Arc, OnceLock, Weak};

use parking_lot::Mutex;
use protocol::client::terminals::{self as client, TerminalClose, TerminalInput, TerminalResize};
use protocol::client::{ApiError, ClientRequest, ErrorCode, HubFrame, ReplyOutcome};
use protocol::terminal::{Numbering, TerminalExit, TerminalFitReply, TerminalSize, TerminalUpdate, Typed, UpdateIndex};
use protocol::worker::{TerminalTyped, WorkerRequest, WorkerResponse};
use tokio::time::timeout;

use super::stand_in::{Client, Connected, WAIT, exited, joined, open_on, output, screen, shown};
use crate::hub::tests::{hub_with_worktree, serving, terminal_in_w};
use crate::hub::{Hub, QUEUED_UPDATES};
use crate::worker_link::WorkerLink;

/// Including a new size that came before the screen, which it took in.
#[tokio::test]
async fn a_client_that_attaches_gets_the_screen_then_only_what_comes_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link) = open_on(dir.path(), |_| Ok(shown(2))).await;
    let mut client = Client::attach(&hub);
    assert_eq!(client.next().await, TerminalUpdate::Screen(screen()));
    let (before, after) = (TerminalSize { cols: 100, rows: 30 }, TerminalSize { cols: 90, rows: 20 });
    let updates = [
        output("old"),
        TerminalUpdate::Resized { size: before },
        output("new"),
        TerminalUpdate::Resized { size: after },
        TerminalUpdate::Unknown,
        exited(),
    ];

    for (at, update) in (0..).zip(updates) {
        hub.terminal_update(&link, "t".into(), UpdateIndex::new(at), update);
    }

    assert_eq!(client.rest().await, [output("new"), TerminalUpdate::Resized { size: after }, exited()]);
}

/// What the worker sends as it's asked what the terminal shows can reach the hub before the answer does, and still
/// comes after the screen, since the client follows the updates before it asks.
#[tokio::test]
async fn an_update_that_reaches_the_hub_before_the_screen_comes_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let reaching: Arc<OnceLock<(Weak<Hub>, Weak<WorkerLink>)>> = Arc::default();
    let sends = reaching.clone();
    let link = WorkerLink::answering(move |request| {
        if matches!(request, WorkerRequest::TerminalShow(_)) {
            let (hub, link) = sends.get().expect("the stand-in to know its hub");
            let (hub, link) = (hub.upgrade().unwrap(), link.upgrade().unwrap());
            hub.terminal_update(&link, "t".into(), UpdateIndex::new(2), output("after"));
            return Some(Ok(shown(2)));
        }
        Some(Ok(WorkerResponse::Ok))
    });
    reaching.set((Arc::downgrade(&hub), Arc::downgrade(&link))).unwrap();
    serving(&hub, &link);
    hub.terminal_open(terminal_in_w("t")).await.unwrap();

    let mut client = Client::attach(&hub);

    assert_eq!(client.next().await, TerminalUpdate::Screen(screen()));
    assert_eq!(client.next().await, output("after"));
}

/// Rather than every update it missed, which nothing holds for it.
#[tokio::test]
async fn a_client_too_far_behind_gets_what_the_terminal_shows_again() {
    let dir = tempfile::tempdir().unwrap();
    let many = u64::try_from(QUEUED_UPDATES).unwrap() + 40;
    let (hub, link) = open_on(dir.path(), move |shows| Ok(shown(if shows == 0 { 0 } else { many }))).await;
    let mut client = Client::attach(&hub);
    assert_eq!(client.next().await, TerminalUpdate::Screen(screen()));

    // All at once, with no await between them, so the client can't keep up.
    for at in 0..many {
        hub.terminal_update(&link, "t".into(), UpdateIndex::new(at), output(&at.to_string()));
    }

    assert_eq!(client.next().await, TerminalUpdate::Screen(screen()));
    hub.terminal_update(&link, "t".into(), UpdateIndex::new(many), output("after"));
    hub.terminal_update(&link, "t".into(), UpdateIndex::new(many + 1), output(" it"));
    hub.terminal_close(TerminalClose { id: "t".into() }).await.unwrap();
    let ended = TerminalUpdate::Exited(TerminalExit { code: None });
    assert_eq!(joined(client.rest().await), ("after it".to_owned(), vec![ended]));
}

/// As its worker has, which takes its shell with it.
#[tokio::test]
async fn a_client_hears_a_terminal_end_once_its_worker_has_gone() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link) = open_on(dir.path(), |_| Ok(shown(0))).await;
    let mut client = Client::attach(&hub);
    assert_eq!(client.next().await, TerminalUpdate::Screen(screen()));

    // As the hub does when a worker has gone.
    hub.terminals_lost(link.id).await;

    assert_eq!(client.rest().await, [TerminalUpdate::Exited(TerminalExit { code: None })]);
}

/// A worker lets go of a terminal only once its shell has ended, which it may not have said yet.
#[tokio::test]
async fn a_terminal_its_worker_has_let_go_of_ends_and_goes() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _link) = open_on(dir.path(), |_| Err(ApiError::new(ErrorCode::NotFound, "gone"))).await;

    let client = Client::attach(&hub);

    assert_eq!(client.rest().await, [TerminalUpdate::Exited(TerminalExit { code: None })]);
    assert_eq!(hub.records_list().await.unwrap().terminals, Vec::new());
}

/// Its shell runs on, so the client waits for a screen, rather than hearing it ended.
#[tokio::test(start_paused = true)]
async fn a_worker_that_cant_say_what_a_terminal_shows_is_asked_again() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _link) =
        open_on(
            dir.path(),
            |shows| if shows == 0 { Err(ApiError::new(ErrorCode::Worker, "no")) } else { Ok(shown(0)) },
        )
        .await;

    let mut client = Client::attach(&hub);

    assert_eq!(client.next().await, TerminalUpdate::Screen(screen()));
    assert_eq!(hub.records_list().await.unwrap().terminals.len(), 1);
}

/// Typing in a burst sends a request for each key, as bytes or as what was pressed, which the connection reads in
/// order, and has to pass on in order, along with new sizes and fits, so the last a client gives is the one the
/// terminal keeps, though each other request runs on a task of its own. On several threads, where those tasks run in
/// any order.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn whats_typed_and_new_sizes_reach_the_worker_in_the_order_they_were_sent() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let reached = Arc::new(Mutex::new(String::new()));
    let reaching = reached.clone();
    let link = WorkerLink::answering(move |request| {
        if let WorkerRequest::TerminalInput(input) = &request {
            reaching.lock().push_str(std::str::from_utf8(input.input.as_slice()).unwrap());
        } else if let WorkerRequest::TerminalTyped(TerminalTyped { typed: Typed::Text { text }, .. }) = &request {
            reaching.lock().push_str(text);
        } else if let WorkerRequest::TerminalResize(resize) = &request {
            write!(reaching.lock(), "{}x,", resize.size.cols).unwrap();
        } else if let WorkerRequest::TerminalFit(fit) = &request {
            write!(reaching.lock(), "{}x,", fit.size.cols).unwrap();
            return Some(Ok(WorkerResponse::TerminalFit(TerminalFitReply {
                numbering: fit.numbering,
                kept: Vec::new(),
            })));
        }
        Some(Ok(WorkerResponse::Ok))
    });
    serving(&hub, &link);
    hub.terminal_open(terminal_in_w("t")).await.unwrap();
    let mut connected = Connected::to(&hub, 1 << 20).await;
    let mut sent = String::new();
    for (id, key) in (1..).zip(0..500_u16) {
        let request = if key % 10 == 0 {
            write!(sent, "{}x,", key + 1).unwrap();
            let size = TerminalSize { cols: key + 1, rows: 24 };
            ClientRequest::TerminalResize(TerminalResize { id: "t".into(), size })
        } else if key % 10 == 5 {
            write!(sent, "{}x,", key + 1).unwrap();
            let size = TerminalSize { cols: key + 1, rows: 24 };
            ClientRequest::TerminalFit(client::TerminalFit {
                id: "t".into(),
                size,
                numbering: Numbering::new(0),
                keep: Vec::new(),
            })
        } else if key % 3 == 0 {
            write!(sent, "{key},").unwrap();
            let typed = Typed::Text { text: format!("{key},") };
            ClientRequest::TerminalTyped(client::TerminalTyped { id: "t".into(), typed })
        } else {
            write!(sent, "{key},").unwrap();
            ClientRequest::TerminalInput(TerminalInput { id: "t".into(), input: format!("{key},").as_bytes().into() })
        };
        connected.send(id, request).await;
    }

    let answered = async {
        for _ in 0..500 {
            let reply = connected.next().await;
            assert!(matches!(reply, HubFrame::Reply { outcome: ReplyOutcome::Response(_), .. }), "{reply:?}");
        }
    };
    timeout(WAIT, answered).await.expect("every request to be answered");
    assert_eq!(*reached.lock(), sent);
}
