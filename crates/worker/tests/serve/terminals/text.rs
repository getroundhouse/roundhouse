//! The ends of a word and the text of cells, which a hub asks a worker of a terminal by number.

use protocol::terminal::{
    Bounds, Numbering, RowNumber, TerminalBoundsReply, TerminalPicture, TerminalPoint, TerminalRange, TerminalTextReply,
};
use protocol::worker::{TerminalBounds, TerminalText, TerminalWatch, WorkerOutcome, WorkerRequest, WorkerResponse};
use tokio::time::Instant;

use super::hub::Hub;

/// The numbering and number of a row the worker sent a picture of whose text is `text`.
fn row_of(pictures: &[(TerminalPicture, Instant)], text: &str) -> Option<(Numbering, RowNumber)> {
    pictures.iter().rev().find_map(|(picture, _)| {
        picture.rows.iter().find_map(|run| {
            let texts =
                run.rows.iter().map(|row| row.row().spans.iter().map(|span| span.text.clone()).collect::<String>());
            (run.from.get()..)
                .zip(texts)
                .find(|(_, row)| row == text)
                .map(|(number, _)| (picture.numbering, RowNumber::new(number)))
        })
    })
}

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
    let (numbering, row) = row_of(&hub.pictures, "hello world").unwrap();

    let at = TerminalPoint { row, col: 8 };
    let bounds = TerminalBounds { terminal: "t".into(), numbering, at, of: Bounds::Word };
    let WorkerOutcome::Response(WorkerResponse::TerminalBounds(TerminalBoundsReply { range: Some(range), .. })) =
        hub.call(WorkerRequest::TerminalBounds(bounds)).await
    else {
        panic!("no word")
    };
    let text = TerminalText { terminal: "t".into(), numbering, range, rectangle: false };
    let copied = hub.call(WorkerRequest::TerminalText(text)).await;

    assert_eq!(range, TerminalRange { from: TerminalPoint { row, col: 6 }, to: TerminalPoint { row, col: 10 } });
    let WorkerOutcome::Response(WorkerResponse::TerminalText(TerminalTextReply { text, rest: None, .. })) = copied
    else {
        panic!("{copied:?}")
    };
    assert_eq!(text, "world");
    hub.finish().await;
}
