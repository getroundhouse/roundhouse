//! What's pressed and new sizes, passed on to a terminal's worker in the order a client sent them.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::terminals as client;
use protocol::client::{CLIENT_PROTOCOL_VERSION, ClientRequest, ClientResponse, HubFrame, ReplyOutcome};
use protocol::terminal::{Numbering, TerminalFitReply, TerminalSize, Typed};
use protocol::worker::{TerminalTyped, WorkerRequest, WorkerResponse};
use tokio::sync::{mpsc, watch};
use tokio::time::timeout;

use super::stand_in::{Connected, WAIT};
use crate::hub::Hub;
use crate::hub::tests::{hub_with_worktree, serving, terminal_in_w};
use crate::server::admit::Greeting;
use crate::server::connection::Connection;
use crate::server::dispatch;
use crate::worker_link::WorkerLink;

/// Typing in a burst sends a request for each key, which the connection reads in order, and has to pass on in order,
/// along with new sizes, so the last a client gives is the one the terminal keeps, though each other request runs on a
/// task of its own. On several threads, where those tasks run in any order.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn whats_typed_and_new_sizes_reach_the_worker_in_the_order_they_were_sent() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, reached) = open_terminal_t(dir.path()).await;
    let mut connected = Connected::to(&hub, 1 << 20).await;
    let mut sent = String::new();
    for (id, key) in (1..).zip(0..500_u16) {
        let request = if key % 5 == 0 {
            write!(sent, "{}x,", key + 1).unwrap();
            fit(key + 1)
        } else {
            write!(sent, "{key},").unwrap();
            typed(&format!("{key},"))
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

/// The dispatch never gets what's pressed or a new size, since the connection sends each on as it reads it. If it
/// did, it would send it on all the same, out of order with the rest, and answer it, rather than leave it unanswered.
#[tokio::test]
async fn the_dispatch_sends_on_and_answers_whats_typed_and_a_new_size() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, reached) = open_terminal_t(dir.path()).await;
    let connection = unread_connection();

    let typed = dispatch::handle(&hub, &connection, greeting(), typed("a")).await;
    let fit = dispatch::handle(&hub, &connection, greeting(), fit(100)).await;

    assert_eq!(typed, Ok(ClientResponse::Ok));
    assert!(matches!(fit, Ok(ClientResponse::TerminalFit(_))), "{fit:?}");
    assert_eq!(*reached.lock(), "a100x,");
}

/// A hub with terminal `t` open, on a worker that writes down what's typed in it, and each new size as `{cols}x,`.
async fn open_terminal_t(dir: &Path) -> (Arc<Hub>, Arc<Mutex<String>>) {
    let hub = hub_with_worktree(dir);
    let reached = Arc::new(Mutex::new(String::new()));
    let reaching = reached.clone();
    let link = WorkerLink::answering(move |request| {
        if let WorkerRequest::TerminalTyped(TerminalTyped { typed: Typed::Text { text }, .. }) = &request {
            reaching.lock().push_str(text);
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
    (hub, reached)
}

/// `text` typed in terminal `t`.
fn typed(text: &str) -> ClientRequest {
    ClientRequest::TerminalTyped(client::TerminalTyped { id: "t".into(), typed: Typed::Text { text: text.into() } })
}

/// Terminal `t` made `cols` wide.
fn fit(cols: u16) -> ClientRequest {
    let size = TerminalSize { cols, rows: 24 };
    ClientRequest::TerminalFit(client::TerminalFit {
        id: "t".into(),
        size,
        numbering: Numbering::new(0),
        keep: Vec::new(),
    })
}

/// A connection for the dispatch to answer on, with nothing reading it.
fn unread_connection() -> Arc<Connection> {
    Arc::new(Connection {
        id: 0,
        frames: mpsc::channel(1).0,
        subscriptions: Mutex::new(HashMap::new()),
        closed: Arc::new(watch::Sender::new(false)),
        closer: crate::server::Closer::default(),
    })
}

/// What a hello from a client of this build, on no device, says.
fn greeting() -> Greeting {
    Greeting { device: None, version: CLIENT_PROTOCOL_VERSION }
}
