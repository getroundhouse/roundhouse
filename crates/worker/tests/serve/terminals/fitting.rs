//! A terminal given a new size by a hub, keeping the cells the hub names.

use protocol::client::ErrorCode;
use protocol::terminal::{
    MOST_KEPT_POINTS, Numbering, RowNumber, TerminalFitReply, TerminalPoint, TerminalRange, TerminalSize,
    TerminalTextReply,
};
use protocol::worker::{TerminalFit, TerminalText, TerminalWatch, WorkerOutcome, WorkerRequest, WorkerResponse};

use super::hub::{Hub, SIZE, refused, row_of};

async fn fit(hub: &mut Hub, size: TerminalSize, numbering: Numbering, keep: Vec<TerminalPoint>) -> WorkerOutcome {
    hub.call(WorkerRequest::TerminalFit(TerminalFit { terminal: "t".into(), size, numbering, keep })).await
}

/// A cell kept through a new size is still the same character, in the numbering the new size starts, and the hub is
/// sent a picture in it though the program writes nothing, as `cat` doesn't on a new size. The same size again starts
/// no new numbering.
#[tokio::test]
async fn a_cell_kept_through_a_new_size_is_the_same_character_in_the_new_numbering() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;
    let watch = TerminalWatch { terminal: "t".into(), watching: true };
    assert_eq!(hub.call(WorkerRequest::TerminalWatch(watch)).await, WorkerOutcome::Response(WorkerResponse::Ok));
    hub.type_in("t", "echo mar''ker; cat").await;
    hub.until_pictures(|pictures| row_of(pictures, "marker").is_some()).await;
    let (numbering, row, col) = row_of(&hub.pictures, "marker").unwrap();
    let k = TerminalPoint { row, col: col + 3 };

    let wider = TerminalSize { cols: SIZE.cols + 20, rows: SIZE.rows };
    let WorkerOutcome::Response(WorkerResponse::TerminalFit(TerminalFitReply { numbering: after, kept })) =
        fit(&mut hub, wider, numbering, vec![k]).await
    else {
        panic!("not fitted")
    };
    let kept = kept[0].unwrap();
    let text = TerminalText {
        terminal: "t".into(),
        numbering: after,
        range: TerminalRange { from: kept, to: kept },
        rectangle: false,
    };
    let copied = hub.call(WorkerRequest::TerminalText(text)).await;
    hub.until_pictures(|pictures| pictures.iter().any(|(picture, _)| picture.numbering == after && picture.whole))
        .await;
    let again = fit(&mut hub, wider, after, vec![kept]).await;

    assert_ne!(after, numbering);
    let WorkerOutcome::Response(WorkerResponse::TerminalText(TerminalTextReply { text, .. })) = copied else {
        panic!("{copied:?}")
    };
    assert_eq!(text, "k");
    let again_fitted = TerminalFitReply { numbering: after, kept: vec![Some(kept)] };
    assert_eq!(again, WorkerOutcome::Response(WorkerResponse::TerminalFit(again_fitted)));
    hub.finish().await;
}

/// A size no terminal can have, and more cells to keep than a fit keeps, are refused, and the terminal keeps its size.
#[tokio::test]
async fn a_fit_too_big_or_keeping_too_much_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub = Hub::start(dir.path()).await;
    hub.open("t", dir.path()).await;
    let numbering = Numbering::new(0);
    let at = TerminalPoint { row: RowNumber::new(0), col: 0 };

    let too_big = fit(&mut hub, TerminalSize { cols: 0, rows: 24 }, numbering, Vec::new()).await;
    let too_much = fit(&mut hub, SIZE, numbering, vec![at; MOST_KEPT_POINTS + 1]).await;

    assert!(refused(&too_big, ErrorCode::InvalidRequest), "{too_big:?}");
    assert!(refused(&too_much, ErrorCode::InvalidRequest), "{too_much:?}");
    hub.finish().await;
}
