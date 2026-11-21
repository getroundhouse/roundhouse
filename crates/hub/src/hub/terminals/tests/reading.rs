//! What a client reads of a terminal's copy, which the hub asks its worker, and waits for only so long.

use std::time::Duration;

use protocol::client::ErrorCode;
use protocol::client::terminals::{TerminalBounds, TerminalRows, TerminalText};
use protocol::terminal::{
    Bounds, Numbering, RowNumber, TerminalBoundsReply, TerminalPoint, TerminalRange, TerminalTextReply,
};
use protocol::worker::{WorkerRequest, WorkerResponse};
use tokio::time::Instant;

use crate::hub::tests::{hub_with_worktree, serving, terminal_in_w};
use crate::worker_link::WorkerLink;

const AT: TerminalPoint = TerminalPoint { row: RowNumber::new(40), col: 3 };

/// The ends of a word and a page of text are the worker's answers, to what the client asked, of the terminal it
/// asked about.
#[tokio::test]
async fn the_ends_of_a_word_and_its_text_are_asked_of_the_worker() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let link = WorkerLink::answering(|request| {
        if let WorkerRequest::TerminalBounds(asked) = &request {
            assert_eq!((asked.terminal.as_str(), asked.at, asked.of), ("t", AT, Bounds::Line));
            let range = Some(TerminalRange { from: asked.at, to: TerminalPoint { col: 9, ..asked.at } });
            return Some(Ok(WorkerResponse::TerminalBounds(TerminalBoundsReply { numbering: asked.numbering, range })));
        }
        if let WorkerRequest::TerminalText(asked) = &request {
            assert!(asked.rectangle);
            let text = format!("from {}", asked.range.from.row.get());
            let reply = TerminalTextReply { numbering: asked.numbering, text, rest: None, joins: false };
            return Some(Ok(WorkerResponse::TerminalText(reply)));
        }
        Some(Ok(WorkerResponse::Ok))
    });
    serving(&hub, &link);
    hub.terminal_open(terminal_in_w("t")).await.unwrap();

    let numbering = Numbering::new(2);
    let bounds =
        hub.terminal_bounds(TerminalBounds { id: "t".into(), numbering, at: AT, of: Bounds::Line }).await.unwrap();
    let range = bounds.range.unwrap();
    let text = hub.terminal_text(TerminalText { id: "t".into(), numbering, range, rectangle: true }).await.unwrap();

    assert_eq!((bounds.numbering, range.to.col), (numbering, 9));
    assert_eq!(text.text, "from 40");
}

/// A worker that doesn't answer what's read of a terminal, as one whose terminal's thread is stuck, fails the read
/// after a while, rather than leaving the client waiting, so it can ask again.
#[tokio::test(start_paused = true)]
async fn a_read_the_worker_doesnt_answer_fails_after_a_while() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_with_worktree(dir.path());
    let link = WorkerLink::answering(|request| {
        (!matches!(request, WorkerRequest::TerminalRows(_))).then_some(Ok(WorkerResponse::Ok))
    });
    serving(&hub, &link);
    hub.terminal_open(terminal_in_w("t")).await.unwrap();
    let began = Instant::now();

    let asked = TerminalRows { id: "t".into(), numbering: Numbering::new(2), from: RowNumber::new(40), count: 10 };
    let failed = hub.terminal_rows(asked).await.unwrap_err();

    assert_eq!(failed.code, ErrorCode::Worker);
    let waited = began.elapsed();
    assert!(waited >= Duration::from_secs(10) && waited < Duration::from_secs(11), "{waited:?}");
}
