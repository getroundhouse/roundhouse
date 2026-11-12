use protocol::event::{EventBody, ExitReason};
use protocol::tail::Tail;

use super::*;

fn exited(session_id: &str, run: u64) -> WorkerFrame {
    let body = EventBody::AgentExited { reason: ExitReason::Requested, code: None, signal: None, stderr_tail: vec![] };
    WorkerFrame::AgentEvent {
        session_id: session_id.into(),
        run: RunId::new(run),
        body,
        raw: None,
        attachments: Vec::new(),
        seq: Some(1),
    }
}

fn writing(session_id: &str, run: u64) -> WorkerFrame {
    WorkerFrame::AgentTail { session_id: session_id.into(), run: RunId::new(run), tail: Tail::default() }
}

/// A replaced run that's still writing is sent after what its replacement said as it exited, and isn't taken for the
/// session's agent.
#[test]
fn a_session_s_agent_is_the_newest_run_its_worker_names_whatever_the_order() {
    let frames = [exited("s", 2), writing("s", 1), writing("t", 3), exited("t", 4)];

    let runs = newest_runs(&frames);

    assert_eq!(runs, HashMap::from([("s".into(), RunId::new(2)), ("t".into(), RunId::new(4))]));
}
