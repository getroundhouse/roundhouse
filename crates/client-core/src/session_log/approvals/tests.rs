use protocol::event::{ApprovalOutcome, NoticeLevel};
use protocol::model::Place;
use serde_json::Value;

use super::*;

fn event(seq: Seq, body: EventBody) -> SessionEvent {
    SessionEvent {
        session_id: "s".into(),
        seq,
        ts_ms: 0,
        run: None,
        subagent: None,
        place: Place::default(),
        body,
        raw: None,
    }
}

fn asked(seq: Seq, id: &str) -> SessionEvent {
    let body = EventBody::ApprovalRequested {
        approval_id: id.into(),
        tool_name: "Bash".into(),
        tool_use_id: None,
        input: Value::Null,
        description: None,
        suggestions: Value::Null,
        asking: None,
    };
    event(seq, body)
}

fn answered(seq: Seq, id: &str) -> SessionEvent {
    let body = EventBody::ApprovalResolved {
        approval_id: id.into(),
        outcome: ApprovalOutcome::Allowed,
        answers_by_text: None,
        answers_by_position: None,
    };
    event(seq, body)
}

fn later(seq: Seq) -> SessionEvent {
    event(seq, EventBody::Notice { level: NoticeLevel::Info, message: "later".into() })
}

fn recorded(open: &[(Seq, &str)], through: Seq) -> RecordedApprovals {
    let open = open.iter().map(|&(seq, id)| OpenApproval { seq, approval_id: ApprovalId::from(id) }).collect();
    RecordedApprovals { open, through }
}

fn seqs(open: &[&SessionEvent]) -> Vec<Seq> {
    open.iter().map(|event| event.seq).collect()
}

/// What's loaded can't say whether an answer is in a page that isn't, and the record can.
#[test]
fn the_record_says_which_are_open_whatever_is_loaded() {
    let mut log = SessionLog::new(0);
    log.apply_event(asked(1, "a"));
    log.apply_event(asked(2, "b"));
    log.apply_event(later(4));

    assert_eq!(seqs(&log.open_approvals(None)), [] as [Seq; 0], "the answer could be the event that isn't here");
    assert_eq!(seqs(&log.open_approvals(Some(&recorded(&[(1, "a")], 4)))), [1], "b's answer is the one");
    assert_eq!(seqs(&log.open_approvals(Some(&recorded(&[], 4)))), [] as [Seq; 0], "none, said");
    assert_eq!(seqs(&log.open_approvals(Some(&recorded(&[(9, "z")], 9)))), [] as [Seq; 0], "not loaded");
}

/// Until the feed brings a newer copy of the record, which says the same.
#[test]
fn what_is_loaded_after_the_record_was_read_says_what_has_changed_since() {
    let mut log = SessionLog::new(0);
    log.apply_event(asked(1, "a"));
    let read_then = recorded(&[(1, "a")], 1);

    log.apply_event(answered(2, "a"));
    log.apply_event(asked(3, "b"));
    log.apply_event(asked(3, "b"));

    assert_eq!(seqs(&log.open_approvals(Some(&read_then))), [3]);
    assert_eq!(seqs(&log.open_approvals(Some(&recorded(&[(3, "b")], 3)))), [3]);
}
