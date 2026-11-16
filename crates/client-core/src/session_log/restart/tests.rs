//! Reading a log again where it is, as from a hub of another build, besides what the laws say of its rows.

use protocol::fixtures::{self, event};
use protocol::model::{ApprovalId, OpenApproval, Seq};

use super::super::pages::page_range;
use super::super::tests::{is_rewritten, page, rewritten};
use super::super::{Position, RecordedApprovals, Row, SessionLog};
use protocol::event::{EventBody, SessionEvent};

/// A request waiting at `seq`, which the tests' requests say is `"a"`, as the session's record says.
fn waiting(seq: Seq) -> RecordedApprovals {
    RecordedApprovals { open: vec![OpenApproval { seq, approval_id: ApprovalId::from("a") }], through: seq }
}

/// The request `"a"` at `seq`, saying `note`, which tells one copy of it from another.
fn asked(seq: Seq, note: &str) -> SessionEvent {
    let mut request = fixtures::approval_request("a", "Bash");
    request.description = Some(note.into());
    event(seq, request.into())
}

/// What the open approvals say, one to each.
fn notes(log: &SessionLog, recorded: &RecordedApprovals) -> Vec<String> {
    let note = |event: &&SessionEvent| {
        let EventBody::ApprovalRequested { description, .. } = &event.body else { panic!("only requests are open") };
        description.clone().unwrap_or_default()
    };
    log.open_approvals(Some(recorded)).iter().map(note).collect()
}

/// A request waiting that no page holds, fetched on its own, is shown until it comes again, then as it comes.
#[test]
fn a_request_fetched_on_its_own_is_fetched_again_and_shown_until_it_comes() {
    let seq = Seq::new(5);
    let mut log = SessionLog::new(Seq::new(600));
    log.waiting_on(Some(&waiting(seq)));
    assert_eq!(log.requests_to_fetch(), vec![seq]);
    log.install_request(seq, Some(asked(seq, "before")));

    log.read_again();

    assert_eq!(notes(&log, &waiting(seq)), ["before"], "it's shown until it comes again");
    assert_eq!(log.requests_to_fetch(), vec![seq]);
    log.install_request(seq, Some(asked(seq, "after")));
    assert_eq!(notes(&log, &waiting(seq)), ["after"]);
    assert!(log.requests_to_fetch().is_empty(), "and isn't fetched a third time");
}

/// One that fails to come again is still fetched again, once retrying is allowed, as on a new connection.
#[test]
fn a_request_that_fails_to_come_again_is_fetched_again_on_retrying() {
    let seq = Seq::new(5);
    let mut log = SessionLog::new(Seq::new(600));
    log.waiting_on(Some(&waiting(seq)));
    log.requests_to_fetch();
    log.install_request(seq, Some(asked(seq, "before")));
    log.read_again();
    log.requests_to_fetch();

    log.install_request(seq, None);
    log.retry_failed();

    assert_eq!(notes(&log, &waiting(seq)), ["before"]);
    assert_eq!(log.requests_to_fetch(), vec![seq]);
}

/// One whose page is let go of before the page came again is fetched again on its own, rather than kept as it was.
#[test]
fn a_request_whose_page_is_let_go_of_before_it_came_again_is_fetched_again() {
    let last = 15_000;
    let mut log = SessionLog::new(Seq::new(last));
    log.waiting_on(Some(&waiting(Seq::FIRST)));
    let mut first = page(1..251);
    first[0] = asked(Seq::FIRST, "before");
    log.focus_on(0..1);
    log.install_page(page_range(0).start, first);
    log.read_again();

    for start in (251..last).step_by(250) {
        let position = Position::of_seq(Seq::new(start));
        log.focus_on(position.row().unwrap()..position.row().unwrap() + 1);
        log.install_page(position, page(start..start + 250));
    }

    assert_eq!(log.row(0), Row::Unloaded, "its page went");
    assert_eq!(notes(&log, &waiting(Seq::FIRST)), ["before"]);
    assert_eq!(log.requests_to_fetch(), vec![Seq::FIRST]);
}

#[test]
fn the_hub_is_asked_for_the_prompts_and_compactions_again() {
    let mut log = SessionLog::new(Seq::new(600));
    log.list_prompts(Vec::new());
    log.list_compactions(Vec::new());
    assert!(!log.wants_prompt_list() && !log.wants_compaction_list());

    log.read_again();

    assert!(log.wants_prompt_list() && log.wants_compaction_list());
}

/// Only the pages up to where the newest come live again are fetched again, since the subscription sends those after.
#[test]
fn the_newest_pages_come_live_and_are_not_fetched_again() {
    let mut log = SessionLog::new(Seq::new(600));
    for event in page(251..601) {
        log.apply_event(event);
    }
    log.focus_on(300..301);
    assert_eq!(log.pages_to_fetch(), vec![page_range(0)]);
    log.install_page(page_range(0).start, page(1..251));

    log.read_again();

    assert_eq!(log.received(), Position::of_seq(Seq::new(250)), "the subscription starts after the first page");
    assert_eq!(log.pages_to_fetch(), vec![page_range(0)]);
    log.install_page(page_range(0).start, rewritten(1..251));
    assert!(matches!(log.row(0), Row::Event(event) if is_rewritten(event)));
    assert!(log.pages_to_fetch().is_empty());
}
