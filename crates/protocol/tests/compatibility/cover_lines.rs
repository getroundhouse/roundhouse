//! The line said for the agent while it can't answer what was said out loud: what its adapter reports, what it claims,
//! and what the tail says of it, which a hub from before reads as a report it doesn't know, and a tail from before as
//! no line.

use protocol::capability::Report;
use protocol::capability::output::{CoverLine, CoverWait};
use protocol::frame;
use protocol::model::RunId;
use protocol::tail::{Covering, Tail};
use protocol::worker::WorkerFrame;
use serde_json::json;

fn report_line(report: &str) -> String {
    format!(r#"{{"frame":"agent_report","session_id":"s","run":4,"report":{report}}}"#)
}

#[test]
fn a_line_and_its_waits_close_keep_their_forms() {
    let line = CoverLine { wait: CoverWait(2), number: 3, text: "Hold on, I'm reading the parser.".into() };
    let said = r#"{"report":"covering","wait":2,"number":3,"text":"Hold on, I'm reading the parser."}"#;
    let closed = r#"{"report":"covered","wait":2}"#;

    for (report, wire) in [(Report::Covering(line), said), (Report::Covered { wait: CoverWait(2) }, closed)] {
        let frame = WorkerFrame::AgentReport { session_id: "s".into(), run: RunId::new(4), report, seq: None };
        assert_eq!(frame::encode(&frame).unwrap(), [report_line(wire).as_bytes(), b"\n"].concat());
        assert_eq!(frame::decode::<WorkerFrame>(report_line(wire).as_bytes()).unwrap(), frame);
    }
}

#[test]
fn a_run_claims_cover_lines_in_its_own_form() {
    let claims: protocol::capability::Capabilities =
        serde_json::from_value(json!({"output":{"cover_lines":{}}})).unwrap();

    assert!(claims.output.cover_lines.is_some());
    assert!(Report::Covered { wait: CoverWait(1) }.allowed(&claims));
}

/// The tail says the line with the run it's for, and one from before says none.
#[test]
fn the_line_in_a_tail_keeps_its_form() {
    let line = CoverLine { wait: CoverWait(1), number: 1, text: "One sec.".into() };
    let tail = Tail { covering: Some(Box::new(Covering { run: Some(RunId::new(4)), line })), ..Tail::default() };
    let wire = json!({"messages":[],"covering":{"run":4,"line":{"wait":1,"number":1,"text":"One sec."}}});

    assert_eq!(serde_json::to_value(&tail).unwrap(), wire);
    assert_eq!(serde_json::from_value::<Tail>(wire).unwrap(), tail);
    assert_eq!(serde_json::from_value::<Tail>(json!({"messages":[]})).unwrap().covering, None);
}
