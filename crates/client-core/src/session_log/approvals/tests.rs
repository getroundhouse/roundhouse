use protocol::event::{ApprovalOutcome, NoticeLevel};
use protocol::fixtures::{self, event};

use super::*;

fn asked(seq: Seq, id: &str) -> SessionEvent {
    event(seq, fixtures::approval_request(id, "Bash").into())
}

fn answered(seq: Seq, id: &str) -> SessionEvent {
    let body = EventBody::ApprovalResolved {
        approval_id: id.into(),
        outcome: ApprovalOutcome::Allowed,
        answers_by_text: None,
        answers_by_position: None,
        by_setting: false,
    };
    event(seq, body)
}

fn later(seq: Seq) -> SessionEvent {
    event(seq, EventBody::Notice { level: NoticeLevel::Info, message: "later".into(), parent_tool_use_id: None })
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
    let mut log = SessionLog::new(Seq::default());
    log.apply_event(asked(Seq::new(1), "a"));
    log.apply_event(asked(Seq::new(2), "b"));
    log.apply_event(later(Seq::new(4)));

    assert_eq!(seqs(&log.open_approvals(None)), [] as [Seq; 0], "the answer could be the event that isn't here");
    assert_eq!(
        seqs(&log.open_approvals(Some(&recorded(&[(Seq::new(1), "a")], Seq::new(4))))),
        [Seq::new(1)],
        "b's answer is the one"
    );
    assert_eq!(seqs(&log.open_approvals(Some(&recorded(&[], Seq::new(4))))), [] as [Seq; 0], "none, said");
    assert_eq!(
        seqs(&log.open_approvals(Some(&recorded(&[(Seq::new(9), "z")], Seq::new(9))))),
        [] as [Seq; 0],
        "not loaded"
    );
}

/// Until the feed brings a newer copy of the record, which says the same.
#[test]
fn what_is_loaded_after_the_record_was_read_says_what_has_changed_since() {
    let mut log = SessionLog::new(Seq::default());
    log.apply_event(asked(Seq::new(1), "a"));
    let read_then = recorded(&[(Seq::new(1), "a")], Seq::new(1));

    log.apply_event(answered(Seq::new(2), "a"));
    log.apply_event(asked(Seq::new(3), "b"));
    log.apply_event(asked(Seq::new(3), "b"));

    assert_eq!(seqs(&log.open_approvals(Some(&read_then))), [Seq::new(3)]);
    assert_eq!(seqs(&log.open_approvals(Some(&recorded(&[(Seq::new(3), "b")], Seq::new(3))))), [Seq::new(3)]);
}

/// A request asked long before, such as by a subagent in the background, can be in a page that isn't loaded.
#[test]
fn a_waiting_request_no_page_holds_is_fetched_on_its_own_once_and_let_go_once_answered() {
    let (z, last) = (Seq::new(9), Seq::new(600));
    let mut log = SessionLog::new(Seq::default());
    log.apply_event(later(last));
    let waiting = recorded(&[(z, "z")], last);
    log.waiting_on(Some(&waiting));
    log.take_changes();

    assert_eq!(log.requests_to_fetch(), [z]);
    assert_eq!(log.requests_to_fetch(), [] as [Seq; 0], "it's on its way");
    log.install_request(z, Some(asked(z, "z")));
    assert_eq!(seqs(&log.open_approvals(Some(&waiting))), [z]);
    let changes = log.take_changes();
    assert!(changes.requests_fetched && changes.rows.is_none(), "a view asks again, and no row changed");
    assert_eq!(log.requests_to_fetch(), [] as [Seq; 0], "it's here");

    log.waiting_on(Some(&recorded(&[], last.next())));
    log.waiting_on(Some(&waiting));
    assert_eq!(log.requests_to_fetch(), [z], "let go of, so fetched again");
}

#[test]
fn a_waiting_request_that_couldnt_be_fetched_waits_for_the_next_connection() {
    let (z, y, last) = (Seq::new(9), Seq::new(10), Seq::new(600));
    let mut log = SessionLog::new(Seq::default());
    log.apply_event(later(last));
    log.waiting_on(Some(&recorded(&[(z, "z"), (y, "y")], last)));

    assert_eq!(log.requests_to_fetch(), [z, y]);
    log.fetch_failed();
    assert_eq!(log.requests_to_fetch(), [] as [Seq; 0]);
    log.retry_failed();
    assert_eq!(log.requests_to_fetch(), [z, y]);
    log.install_request(z, None);
    log.install_request(y, Some(asked(z, "z")));
    assert!(log.open_approvals(Some(&recorded(&[(z, "z"), (y, "y")], last))).is_empty(), "nor is one not asked for");
    assert_eq!(log.requests_to_fetch(), [] as [Seq; 0], "until the next connection");
    log.retry_failed();
    assert_eq!(log.requests_to_fetch(), [z, y]);

    // One fetched after it no longer waits is let go of, and fetched again if it waits again.
    log.waiting_on(Some(&recorded(&[], last.next())));
    log.install_request(z, Some(asked(z, "z")));
    log.waiting_on(Some(&recorded(&[(z, "z")], last.next())));
    assert_eq!(log.requests_to_fetch(), [z]);
}

/// Its page can be let go of as a view looks elsewhere, and what waits is still asked.
#[test]
fn a_waiting_request_whose_page_is_let_go_of_is_kept() {
    let last = 15_000;
    let mut log = SessionLog::new(Seq::new(last));
    let waiting = recorded(&[(Seq::FIRST, "a")], Seq::new(last));
    log.waiting_on(Some(&waiting));
    for start in (1..last).step_by(250) {
        let at = |seq| if seq == 1 { asked(Seq::FIRST, "a") } else { later(Seq::new(seq)) };
        let position = Position::of_seq(Seq::new(start));
        log.focus_on(position.row().unwrap());
        log.install_page(position, (start..start + 250).map(at).collect());
    }

    assert_eq!(log.row(0), crate::session_log::Row::Unloaded, "its page went");
    assert_eq!(seqs(&log.open_approvals(Some(&waiting))), [Seq::FIRST]);
    assert_eq!(log.requests_to_fetch(), [] as [Seq; 0], "and it isn't fetched again");
}
