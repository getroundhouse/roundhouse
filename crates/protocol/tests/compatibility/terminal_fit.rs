//! A terminal given a new size, keeping the cells a client names, as a client asks its hub and the hub its worker,
//! and where each went.

use protocol::client::terminals::TerminalFit;
use protocol::client::{ClientFrame, ClientRequest, ClientResponse, HubFrame, ReplyOutcome};
use protocol::terminal::{Numbering, RowNumber, TerminalFitReply, TerminalPoint, TerminalSize};
use protocol::worker::{self, WorkerRequest, WorkerResponse};

use crate::keeps_its_form;
use crate::worker_calls::{answered, asked};

const SIZE: TerminalSize = TerminalSize { cols: 120, rows: 40 };
const TOP: TerminalPoint = TerminalPoint { row: RowNumber::new(40), col: 0 };

#[test]
fn a_new_size_and_the_cells_it_keeps_keep_their_form() {
    let fit = TerminalFit { id: "t".into(), size: SIZE, numbering: Numbering::new(2), keep: vec![TOP] };
    keeps_its_form(
        &ClientFrame { id: 7, request: ClientRequest::TerminalFit(fit) },
        concat!(
            r#"{"id":7,"request":{"method":"terminal_fit","id":"t","size":{"cols":120,"rows":40},"numbering":2,"#,
            r#""keep":[{"row":40,"col":0}]}}"#,
        ),
    );
    let fit = worker::TerminalFit { terminal: "t".into(), size: SIZE, numbering: Numbering::new(2), keep: Vec::new() };
    asked(
        WorkerRequest::TerminalFit(fit),
        r#"{"method":"terminal_fit","terminal":"t","size":{"cols":120,"rows":40},"numbering":2}"#,
    );
}

/// Where each cell went, and one that wasn't kept, from the worker and from the hub.
#[test]
fn where_the_cells_went_keeps_its_form() {
    let fitted = TerminalFitReply { numbering: Numbering::new(3), kept: vec![Some(TOP), None] };
    let form = r#"{"result":"terminal_fit","numbering":3,"kept":[{"row":40,"col":0},null]}"#;

    answered(WorkerResponse::TerminalFit(fitted.clone()), form);
    keeps_its_form(
        &HubFrame::Reply { id: 7, outcome: ReplyOutcome::Response(ClientResponse::TerminalFit(fitted)) },
        &format!(r#"{{"frame":"reply","id":7,"response":{form}}}"#),
    );
}
