use super::*;
use protocol::event::{ApprovalOutcome, NoticeLevel};
use protocol::model::Place;
use protocol::tail::DeltaChange;
use serde_json::Value;

fn notice(seq: Seq) -> SessionEvent {
    SessionEvent {
        session_id: "s".into(),
        seq,
        ts_ms: 0,
        run: None,
        subagent: None,
        place: Place::default(),
        body: EventBody::Notice { level: NoticeLevel::Info, message: format!("{seq}") },
        raw: None,
    }
}

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

pub(super) fn page(range: Range<Seq>) -> Vec<SessionEvent> {
    range.map(notice).collect()
}

#[test]
fn rows_are_indexed_by_seq_whatever_is_loaded() {
    let mut log = SessionLog::new(1000);
    assert_eq!(log.row_count(), 1000);
    assert_eq!(log.row(0), Row::Unloaded);
    log.focus_on(300);
    log.install_page(251, page(251..501));
    assert!(matches!(log.row(250), Row::Event(e) if e.seq == 251));
    assert_eq!(log.row(249), Row::Unloaded);
    assert_eq!(log.row(500), Row::Unloaded);
}

#[test]
fn it_asks_for_the_page_in_view_first_and_only_once() {
    let mut log = SessionLog::new(1000);
    log.focus_on(300);
    let first = log.pages_to_fetch();
    assert_eq!(first, vec![251..501, 1..251], "501 on is on its way from the subscription");
    assert!(log.pages_to_fetch().is_empty(), "already in flight");

    log.fetch_failed();
    assert!(log.pages_to_fetch().is_empty(), "a failed fetch isn't asked for again at once");
    log.retry_failed();
    assert_eq!(log.pages_to_fetch(), first, "but is once retrying is allowed, all of it");

    assert!(SessionLog::new(0).pages_to_fetch().is_empty());
}

#[test]
fn an_installed_page_reports_the_rows_it_filled_and_is_not_fetched_again() {
    let mut log = SessionLog::new(800);
    log.focus_on(300);
    assert_eq!(log.pages_to_fetch(), vec![251..501, 1..251]);
    log.install_page(1, page(1..251));
    assert_eq!(log.take_changes().rows, Some(0..250));
    log.install_page(251, page(251..501));
    assert_eq!(log.take_changes().rows, Some(250..500));
    assert!(log.pages_to_fetch().is_empty());
}

#[test]
fn a_short_last_page_fills_only_the_rows_there_are() {
    let mut log = SessionLog::new(300);
    for event in page(1..301) {
        log.apply_event(event);
    }
    log.take_changes();
    log.install_page(251, page(251..301));
    assert_eq!(log.take_changes().rows, Some(250..300));
}

#[test]
fn it_never_holds_a_whole_long_session() {
    let mut log = SessionLog::new(15_000);
    for start in (1..15_000).step_by(seq_index(PAGE_EVENTS)) {
        log.focus_on(seq_index(start) - 1);
        log.install_page(start, page(start..start + PAGE_EVENTS));
        assert!(log.loaded_events() <= MAX_LOADED_EVENTS + seq_index(PAGE_EVENTS));
    }
    // What's near the view and the tail stayed, and what was dropped can be fetched again.
    assert!(matches!(log.row(14_999), Row::Event(_)));
    assert_eq!(log.row(0), Row::Unloaded);
    log.focus_on(0);
    assert_eq!(log.pages_to_fetch(), vec![1..251, 251..501]);
}

#[test]
fn live_events_never_build_up_past_the_limit() {
    let mut log = SessionLog::new(0);
    for event in page(1..10_001) {
        log.apply_event(event);
    }
    assert!(log.loaded_events() <= MAX_LOADED_EVENTS + seq_index(PAGE_EVENTS), "{}", log.loaded_events());
    assert!(matches!(log.row(9_999), Row::Event(_)));
}

#[test]
fn a_prompt_shows_at_once_then_becomes_the_recorded_event() {
    let mut log = SessionLog::new(5);
    log.add_pending("m1".into(), "hello".into(), Vec::new());
    assert_eq!(log.row_count(), 6);
    assert!(matches!(log.row(5), Row::Pending(p) if p.text == "hello"));

    log.apply_event(event(
        6,
        EventBody::UserPrompt { text: "hello".into(), client_msg_id: Some("m1".into()), attachments: Vec::new() },
    ));
    assert_eq!(log.row_count(), 6, "it swapped in place");
    assert!(matches!(log.row(5), Row::Event(_)));
}

#[test]
fn a_prompt_that_could_not_be_sent_can_be_taken_back() {
    let mut log = SessionLog::new(0);
    log.add_pending("m1".into(), "hello".into(), Vec::new());
    assert_eq!(log.remove_pending(&"m1".into()).unwrap().text, "hello");
    assert_eq!(log.row_count(), 0);
    assert_eq!(log.remove_pending(&"m1".into()), None);
}

#[test]
fn live_events_grow_the_log_and_replays_do_not() {
    let mut log = SessionLog::new(2);
    log.apply_event(notice(3));
    log.apply_event(notice(3));
    assert_eq!(log.row_count(), 3);
}

#[test]
fn open_approvals_are_those_not_yet_answered() {
    let requested = |id: &str| EventBody::ApprovalRequested {
        approval_id: id.into(),
        tool_name: "Bash".into(),
        tool_use_id: None,
        input: Value::Null,
        description: None,
        suggestions: Value::Null,
    };
    let mut log = SessionLog::new(0);
    log.apply_event(event(1, requested("a")));
    log.apply_event(event(2, requested("b")));
    log.apply_event(event(
        3,
        EventBody::ApprovalResolved { approval_id: "a".into(), outcome: ApprovalOutcome::Allowed },
    ));
    let open = log.open_approvals();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].seq, 2);
}

#[test]
fn the_streaming_tail_follows_deltas_and_clears_with_the_finished_message() {
    let mut log = SessionLog::new(0);
    log.apply_delta(&StreamDelta {
        run: None,
        message_id: "m".into(),
        parent_tool_use_id: None,
        change: DeltaChange::Text { index: 0, text: "hi".into() },
    });
    assert!(!log.tail().is_empty());
    log.apply_event(event(
        1,
        EventBody::Assistant {
            message_id: Some("m".into()),
            parent_tool_use_id: None,
            model: None,
            blocks: vec![],
            first_index: None,
        },
    ));
    assert!(log.tail().is_empty());
}

#[test]
fn changes_gather_into_one_range_until_a_view_looks() {
    let mut log = SessionLog::new(0);
    log.add_pending("p".into(), "hi".into(), Vec::new());
    log.apply_event(event(
        1,
        EventBody::UserPrompt { text: "hi".into(), client_msg_id: Some("p".into()), attachments: Vec::new() },
    ));
    log.apply_event(event(
        2,
        EventBody::TurnEnded {
            outcome: protocol::event::TurnOutcome::Success,
            result: None,
            errors: vec![],
            duration_ms: None,
            cost_usd: None,
            num_turns: None,
            started_at_ms: None,
        },
    ));

    let changes = log.take_changes();

    assert_eq!(changes, Changes { rows: Some(0..3), turns_ended: 1 }, "two rows and the tail");
    assert_eq!(log.take_changes(), Changes::default(), "nothing since");
}
