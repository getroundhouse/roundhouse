//! What a client asks of a terminal's cells, the ends of a word or a line and the text between two cells, as it asks
//! its hub and the hub the worker, and what each answers.

use protocol::client::terminals::{TerminalBounds, TerminalText};
use protocol::client::{ClientFrame, ClientRequest, ClientResponse, HubFrame, ReplyOutcome};
use protocol::terminal::{
    Bounds, Numbering, RowNumber, TerminalBoundsReply, TerminalPoint, TerminalRange, TerminalTextReply,
};
use protocol::worker::{self, WorkerRequest, WorkerResponse};

use crate::keeps_its_form;
use crate::worker_calls::{answered, asked};

fn point(row: u64, col: u16) -> TerminalPoint {
    TerminalPoint { row: RowNumber::new(row), col }
}

const RANGE: TerminalRange = TerminalRange {
    from: TerminalPoint { row: RowNumber::new(40), col: 3 },
    to: TerminalPoint { row: RowNumber::new(41), col: 9 },
};

#[test]
fn what_a_client_asks_of_a_terminals_cells_keeps_its_form() {
    let client_asked = |request: ClientRequest, form: &str| {
        keeps_its_form(&ClientFrame { id: 7, request }, &format!(r#"{{"id":7,"request":{form}}}"#));
    };

    let bounds = TerminalBounds { id: "t".into(), numbering: Numbering::new(2), at: point(40, 3), of: Bounds::Word };
    client_asked(
        ClientRequest::TerminalBounds(bounds),
        r#"{"method":"terminal_bounds","id":"t","numbering":2,"at":{"row":40,"col":3},"of":"word"}"#,
    );
    let text = TerminalText { id: "t".into(), numbering: Numbering::new(2), range: RANGE, rectangle: true };
    client_asked(
        ClientRequest::TerminalText(text),
        concat!(
            r#"{"method":"terminal_text","id":"t","numbering":2,"#,
            r#""range":{"from":{"row":40,"col":3},"to":{"row":41,"col":9}},"rectangle":true}"#,
        ),
    );
}

#[test]
fn what_a_hub_asks_of_a_workers_terminal_cells_keeps_its_form() {
    let bounds = worker::TerminalBounds {
        terminal: "t".into(),
        numbering: Numbering::new(2),
        at: point(40, 3),
        of: Bounds::Line,
    };
    asked(
        WorkerRequest::TerminalBounds(bounds),
        r#"{"method":"terminal_bounds","terminal":"t","numbering":2,"at":{"row":40,"col":3},"of":"line"}"#,
    );
    let text =
        worker::TerminalText { terminal: "t".into(), numbering: Numbering::new(2), range: RANGE, rectangle: false };
    asked(
        WorkerRequest::TerminalText(text),
        concat!(
            r#"{"method":"terminal_text","terminal":"t","numbering":2,"#,
            r#""range":{"from":{"row":40,"col":3},"to":{"row":41,"col":9}}}"#,
        ),
    );
}

/// The ends found, none, a page with more to come on the same line, and the last page, from the worker and from the
/// hub.
#[test]
fn what_a_worker_and_a_hub_answer_of_a_terminals_cells_keeps_its_form() {
    let found = TerminalBoundsReply { numbering: Numbering::new(2), range: Some(RANGE) };
    let none = TerminalBoundsReply { numbering: Numbering::new(3), range: None };
    let page =
        TerminalTextReply { numbering: Numbering::new(2), text: "ab".into(), rest: Some(point(41, 0)), joins: true };
    let last = TerminalTextReply { numbering: Numbering::new(2), text: "c".into(), rest: None, joins: false };

    let found_form =
        r#"{"result":"terminal_bounds","numbering":2,"range":{"from":{"row":40,"col":3},"to":{"row":41,"col":9}}}"#;
    let none_form = r#"{"result":"terminal_bounds","numbering":3}"#;
    let page_form = r#"{"result":"terminal_text","numbering":2,"text":"ab","rest":{"row":41,"col":0},"joins":true}"#;
    let last_form = r#"{"result":"terminal_text","numbering":2,"text":"c"}"#;
    answered(WorkerResponse::TerminalBounds(found.clone()), found_form);
    answered(WorkerResponse::TerminalBounds(none.clone()), none_form);
    answered(WorkerResponse::TerminalText(page.clone()), page_form);
    answered(WorkerResponse::TerminalText(last.clone()), last_form);

    let replied = |response: ClientResponse, form: &str| {
        let reply = HubFrame::Reply { id: 7, outcome: ReplyOutcome::Response(response) };
        keeps_its_form(&reply, &format!(r#"{{"frame":"reply","id":7,"response":{form}}}"#));
    };
    replied(ClientResponse::TerminalBounds(found), found_form);
    replied(ClientResponse::TerminalBounds(none), none_form);
    replied(ClientResponse::TerminalText(page), page_form);
    replied(ClientResponse::TerminalText(last), last_form);
}

/// What a newer client asks the ends of has none, and what's copied stays out of a log.
#[test]
fn ends_a_newer_peer_asks_for_still_read_and_copied_text_is_left_out_of_debug() {
    let read = serde_json::from_str::<Bounds>(r#""paragraph""#).unwrap();
    let copied = TerminalTextReply { numbering: Numbering::new(2), text: "s3cret".into(), rest: None, joins: false };

    assert_eq!(read, Bounds::Unknown);
    assert!(!format!("{copied:?}").contains("s3cret"), "{copied:?}");
}
