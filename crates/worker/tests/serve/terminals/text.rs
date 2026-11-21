//! The ends of a word and the text of cells, which a hub asks a worker of a terminal by number.

use protocol::terminal::{Bounds, TerminalBoundsReply, TerminalPoint, TerminalRange, TerminalTextReply};
use protocol::worker::{TerminalBounds, TerminalText, TerminalWatch, WorkerOutcome, WorkerRequest, WorkerResponse};

use super::hub::{Hub, row_of};

/// A word a program wrote, found by a cell of it in the row the worker numbered, and copied, as a double click would.
#[tokio::test]
async fn a_word_is_found_and_copied_by_its_rows_number() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;
    let watch = TerminalWatch { terminal: "t".into(), watching: true };
    assert_eq!(hub.call(WorkerRequest::TerminalWatch(watch)).await, WorkerOutcome::Response(WorkerResponse::Ok));
    hub.type_in("t", "echo hel''lo world").await;
    hub.until_pictures(|pictures| row_of(pictures, "hello world").is_some()).await;
    let (numbering, row, col) = row_of(&hub.pictures, "hello world").unwrap();

    let at = TerminalPoint { row, col: col + 8 };
    let bounds = TerminalBounds { terminal: "t".into(), numbering, at, of: Bounds::Word };
    let WorkerOutcome::Response(WorkerResponse::TerminalBounds(TerminalBoundsReply { range: Some(range), .. })) =
        hub.call(WorkerRequest::TerminalBounds(bounds)).await
    else {
        panic!("no word")
    };
    let text = TerminalText { terminal: "t".into(), numbering, range, rectangle: false };
    let copied = hub.call(WorkerRequest::TerminalText(text)).await;

    let word = TerminalRange { from: TerminalPoint { row, col: col + 6 }, to: TerminalPoint { row, col: col + 10 } };
    assert_eq!(range, word);
    let WorkerOutcome::Response(WorkerResponse::TerminalText(TerminalTextReply { text, rest: None, .. })) = copied
    else {
        panic!("{copied:?}")
    };
    assert_eq!(text, "world");
    hub.finish().await;
}
