//! What's pressed and new sizes, passed on to a terminal's worker in the order a client sent them.

use std::fmt::Write as _;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::terminals as client;
use protocol::client::{ClientRequest, HubFrame, ReplyOutcome};
use protocol::terminal::{Numbering, TerminalFitReply, TerminalSize, Typed};
use protocol::worker::{TerminalTyped, WorkerRequest, WorkerResponse};
use tokio::time::timeout;

use super::stand_in::{Connected, WAIT};
use crate::hub::tests::{hub_with_worktree, serving, terminal_in_w};
use crate::worker_link::WorkerLink;

/// Typing in a burst sends a request for each key, which the connection reads in order, and has to pass on in order,
/// along with new sizes, so the last a client gives is the one the terminal keeps, though each other request runs on a
/// task of its own. On several threads, where those tasks run in any order.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn whats_typed_and_new_sizes_reach_the_worker_in_the_order_they_were_sent() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
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
    let mut connected = Connected::to(&hub, 1 << 20).await;
    let mut sent = String::new();
    for (id, key) in (1..).zip(0..500_u16) {
        let request = if key % 5 == 0 {
            write!(sent, "{}x,", key + 1).unwrap();
            let size = TerminalSize { cols: key + 1, rows: 24 };
            ClientRequest::TerminalFit(client::TerminalFit {
                id: "t".into(),
                size,
                numbering: Numbering::new(0),
                keep: Vec::new(),
            })
        } else {
            write!(sent, "{key},").unwrap();
            let typed = Typed::Text { text: format!("{key},") };
            ClientRequest::TerminalTyped(client::TerminalTyped { id: "t".into(), typed })
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
