//! Opening a file and reading its pages, on hubs scripted for what a real hub can't be made to do on cue.

use std::ops::Range;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use protocol::client::files::WorkspaceFilePages;
use protocol::client::{ClientRequest, ClientResponse, ErrorCode, ReplyOutcome};
use protocol::model::{CommitId, FileOpened, FilePage, FilePages, OpenedText, PageLine, WorkspaceId};

use super::{OpenFile, Opening};
use crate::client::Client;
use crate::client::open_session::tests::{refused, start_hub_answering};

/// How many lines the files here have, and a page of them.
const LINES: u32 = 600;
const PAGE_LINES: u32 = 256;

/// A file of [`LINES`] lines whose line `n` says `n`, opened with none of its pages, and read a page at a time.
fn answered(request: &ClientRequest) -> ReplyOutcome {
    #[expect(clippy::wildcard_enum_match_arm, reason = "the scripted hub takes every other request")]
    let response = match request {
        ClientRequest::WorkspaceFileOpenAt(_) => {
            let opened = OpenedText { bytes: 0, lines: LINES, page_lines: PAGE_LINES, pages: Vec::new() };
            ClientResponse::FileOpened(FileOpened::Text(opened))
        }
        ClientRequest::WorkspaceFilePages(WorkspaceFilePages { pages, .. }) => {
            ClientResponse::FilePages(FilePages { pages: pages.iter().map(|index| page(*index)).collect() })
        }
        _ => ClientResponse::Ok,
    };
    ReplyOutcome::Response(response)
}

fn page(index: u32) -> FilePage {
    let first = index * PAGE_LINES + 1;
    let lines = (first..(first + PAGE_LINES).min(LINES + 1)).map(|n| PageLine { text: n.to_string(), cut: None });
    FilePage { index, lines: lines.collect() }
}

fn start(socket: &Path) -> OpenFile {
    OpenFile::start(&Client::connect(socket, "test").unwrap(), &WorkspaceId::from("w"), &CommitId::from("c"), "f")
}

/// Waits for `open` to say it changed, until `done` holds of what it holds.
fn wait_for(open: &OpenFile, what: &str, done: impl Fn(&Opening) -> bool) {
    while !open.read(&done) {
        open.changed().recv_timeout(Duration::from_secs(10)).unwrap_or_else(|_| panic!("waiting for {what}"));
    }
}

/// Line `number`, if it's held.
fn line(opening: &Opening, number: u32) -> Option<String> {
    let Opening::Text(text) = opening else { return None };
    text.line(number).map(|line| line.text.clone())
}

/// One run of lines, by number, as a view asks for them.
fn run(lines: Range<u32>) -> [Range<u32>; 1] {
    [lines]
}

/// Polls `done` until it holds, failing after a while.
fn poll(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_pages_a_view_wants_are_fetched_once_and_it_hears_when_they_come() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let asked = start_hub_answering(&socket, |request| Some(answered(request)));
    let open = start(&socket);
    wait_for(&open, "the file to open", |opening| matches!(opening, Opening::Text(_)));

    open.want(&run(300..310));
    open.want(&run(305..320));

    wait_for(&open, "the page", |opening| line(opening, 300).is_some());
    let pages: Vec<Vec<u32>> = asked
        .try_iter()
        .filter_map(
            |request| if let ClientRequest::WorkspaceFilePages(asked) = request { Some(asked.pages) } else { None },
        )
        .collect();
    assert_eq!(pages, [vec![1]], "one page, asked for once");
    assert_eq!(open.read(|opening| (line(opening, 320), line(opening, 1))), (Some("320".to_string()), None));
}

#[test]
fn a_file_that_couldnt_be_opened_is_opened_on_the_next_connection_but_not_on_a_hub_too_old() {
    let dir = tempfile::tempdir().unwrap();
    let (failing, older, newer) = (dir.path().join("1.sock"), dir.path().join("2.sock"), dir.path().join("3.sock"));
    start_hub_answering(&failing, |_| Some(refused(ErrorCode::Internal, "no")));
    start_hub_answering(&older, |_| Some(refused(ErrorCode::Unsupported, "unknown request")));
    start_hub_answering(&newer, |request| Some(answered(request)));
    let (failed, old) = (start(&failing), start(&older));

    wait_for(&failed, "it to fail", |opening| matches!(opening, Opening::Failed));
    wait_for(&old, "the hub to say it's too old", |opening| matches!(opening, Opening::Unsupported));
    assert!(failed.reconnect(&Client::connect(&newer, "test").unwrap()), "opened again");
    assert!(!old.reconnect(&Client::connect(&newer, "test").unwrap()), "not opened again");

    wait_for(&failed, "it to open", |opening| matches!(opening, Opening::Text(_)));
    assert_eq!(failed.take_errors().len(), 1, "what went wrong the first time");
    assert!(old.read(|opening| matches!(opening, Opening::Unsupported)), "not asked again");
}

#[test]
fn a_page_that_couldnt_be_fetched_is_fetched_when_its_next_wanted() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("hub.sock");
    let failed_once = Arc::new(AtomicBool::new(false));
    let failing = failed_once.clone();
    start_hub_answering(&socket, move |request| {
        let fails = matches!(request, ClientRequest::WorkspaceFilePages(_)) && !failing.swap(true, Ordering::SeqCst);
        Some(if fails { refused(ErrorCode::Internal, "no") } else { answered(request) })
    });
    let open = start(&socket);
    wait_for(&open, "the file to open", |opening| matches!(opening, Opening::Text(_)));

    open.want(&run(1..2));
    poll("the page to fail", || failed_once.load(Ordering::SeqCst) && !open.take_errors().is_empty());
    open.want(&run(1..2));

    wait_for(&open, "the page, the second time", |opening| line(opening, 1).is_some());
}
