//! How what's counted follows what the log holds, and what the bar says of it.

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use client_core::session_log::SessionLog;
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::find::Needle;
use protocol::model::{Place, RecentWork, Rev, Seq, Subagent, SubagentStatus};
use serde_json::json;

use super::count::{At, Counts};
use super::stretch::{Records, counted_in};
use super::{More, changed_to_count, count_held, first_match, held_to_count, says};
use crate::fixtures::{calls_to, event, log, within};
use crate::transcript::Model;

fn needle(query: &str) -> Needle {
    Needle::new(query).unwrap()
}

fn agent_says(text: &str) -> EventBody {
    let blocks = vec![ContentBlock::Text { text: text.into(), images: Vec::new() }];
    EventBody::Assistant { message_id: None, parent_tool_use_id: None, model: None, blocks, first_index: None }
}

fn runs(id: &str, command: &str) -> EventBody {
    let blocks =
        vec![ContentBlock::ToolUse { id: id.into(), name: "Bash".into(), input: json!({ "command": command }) }];
    EventBody::Assistant { message_id: None, parent_tool_use_id: None, model: None, blocks, first_index: None }
}

fn printed(id: &str, text: &str) -> EventBody {
    let block = ContentBlock::ToolResult {
        tool_use_id: id.into(),
        is_error: false,
        text: text.into(),
        images: Vec::new(),
        change: None,
        encrypted: false,
    };
    EventBody::AgentUser { parent_tool_use_id: None, blocks: vec![block] }
}

/// `body` at `place` among the agent's own events, with a `seq` far from it, as the agent's are once a subagent has
/// worked.
fn placed(place: u64, body: EventBody) -> SessionEvent {
    SessionEvent { place: Place::new(place), ..event(place * 3, body) }
}

/// The agent's own events, by place, as a chat follows them.
fn own(events: impl IntoIterator<Item = SessionEvent>) -> SessionLog {
    let mut log = SessionLog::by_place(0);
    for event in events {
        log.apply_event(event);
    }
    log
}

/// The record of the subagent the call `call` started, done, with `report`.
fn reported(call: &str, report: &str) -> Records {
    let record = Subagent {
        session_id: "s".into(),
        call: call.into(),
        kind: None,
        description: None,
        prompt: None,
        in_background: true,
        model: None,
        status: SubagentStatus::Finished,
        started_at_ms: 0,
        ended_at_ms: Some(1),
        calls: 1,
        latest_call: None,
        recent: RecentWork::default(),
        report: Some(report.into()),
        rev: Rev::default(),
    };
    HashMap::from([(call.into(), record)])
}

/// Whatever else happened, so the events that matter are on pages of their own.
fn filler(seqs: std::ops::Range<Seq>) -> impl Iterator<Item = SessionEvent> {
    seqs.map(|seq| event(seq, agent_says("nothing to see")))
}

/// Counts `rows` of `log` into `counts`, as the transcript does.
fn count(log: &SessionLog, m: &Model, needle: &Needle, rows: BTreeSet<usize>, counts: &mut Counts) {
    for (row, count, _) in count_held(log, m, needle, rows) {
        counts.set(row, count);
    }
}

/// Counts again what changes when `changed` rows of `log` do, as the transcript does.
fn follow(
    log: &SessionLog,
    m: &Model,
    needle: &Needle,
    candidates: &mut BTreeSet<Seq>,
    counts: &mut Counts,
    changed: std::ops::Range<usize>,
) {
    let (found, rows) = changed_to_count(log, m, (needle, candidates, counts), changed);
    candidates.extend(found);
    count(log, m, needle, rows, counts);
}

#[test]
fn what_the_log_holds_is_counted_by_the_rows_that_draw_it() {
    let log = own([
        placed(1, agent_says("the lexer")),
        placed(2, agent_says("the parser")),
        placed(3, runs("run", "cargo test")),
    ]);
    let m = Model::default();

    let (found, rows) = held_to_count(&log, &m, &needle("parser"));

    assert_eq!(found, [2], "by its place, not its seq");
    assert_eq!(rows.into_iter().collect::<Vec<_>>(), [1], "only the rows of what holds it");
}

#[test]
fn a_result_that_comes_after_its_call_is_counted_in_the_call_s_row() {
    let needle = needle("failed");
    let m = Model::default();
    let (mut candidates, mut counts) = (BTreeSet::new(), Counts::default());
    let before = log([event(1, runs("run", "cargo test"))]);
    follow(&before, &m, &needle, &mut candidates, &mut counts, 0..1);
    assert_eq!(counts.total(), 0);

    let after = log([event(1, runs("run", "cargo test")), event(2, printed("run", "test parser ... FAILED"))]);
    follow(&after, &m, &needle, &mut candidates, &mut counts, 1..2);

    assert_eq!(counts.of(0), 1, "the call draws what came back");
    assert_eq!(counts.total(), 1);
}

#[test]
fn a_subagent_s_work_held_before_its_call_moves_into_the_call_once_it_loads() {
    let needle = needle("parser");
    let m = Model::default();
    let work = || event(300, within("helper", agent_says("Looking at the parser")));
    let (mut candidates, mut counts) = (BTreeSet::new(), Counts::default());
    // Its page loads first, without the call, so it's drawn in its own row.
    follow(&log([work()]), &m, &needle, &mut candidates, &mut counts, 299..300);
    assert_eq!(counts.of(299), 1);

    // Then the call's page, which draws it, far from where its page changed.
    let both = log(std::iter::once(event(1, calls_to("Agent", &["helper"]))).chain(filler(2..300)).chain([work()]));
    follow(&both, &m, &needle, &mut candidates, &mut counts, 0..250);

    assert_eq!((counts.of(0), counts.of(299)), (1, 0), "in the call now, and not where it was");
    assert_eq!(counts.total(), 1, "once");
}

#[test]
fn a_subagent_s_call_counts_the_report_its_record_has_as_it_draws_it() {
    let needle = needle("parser");
    let call = || placed(1, calls_to("Agent", &["helper"]));

    let read = counted_in(vec![call()], &[1], &needle, reported("helper", "The parser is fixed"));
    let subagents = reported("helper", "The parser is fixed").into_iter().map(|(call, s)| (call, Rc::new(s)));
    let m = Model { subagents: subagents.collect(), ..Model::default() };
    let mut counts = Counts::default();
    count(&own([call()]), &m, &needle, [0].into(), &mut counts);

    assert_eq!(read, [(0, 1)], "read in a stretch");
    assert_eq!(counts.of(0), 1, "and held");
    assert!(counted_in(vec![call()], &[1], &needle, HashMap::new()).iter().all(|(_, n)| *n == 0), "with no report");
}

#[test]
fn a_row_the_log_lets_go_of_keeps_what_it_was_counted_as() {
    let needle = needle("parser");
    let mut counts = Counts::default();
    counts.set(4, 2);
    let held = log([event(1, agent_says("the parser"))]);

    follow(&held, &Model::default(), &needle, &mut BTreeSet::new(), &mut counts, 0..10);

    assert_eq!((counts.of(0), counts.of(4)), (1, 2));
}

#[test]
fn a_stretch_counts_what_came_back_in_its_call_s_row_by_place() {
    let events = vec![placed(1, runs("run", "cargo test")), placed(2, printed("run", "test parser ... FAILED"))];

    let counted = counted_in(events, &[2], &needle("failed"), HashMap::new());

    assert_eq!(counted, [(0, 1), (1, 0)]);
}

#[test]
fn the_first_match_is_shown_once_nothing_above_it_can_hold_another() {
    let mut counts = Counts::default();
    counts.set(10, 1);
    counts.set(50, 1);
    let around_it = log((6..=11).map(|seq| event(seq, agent_says("nothing to see"))));
    let only_it = log([event(11, agent_says("nothing to see"))]);

    assert_eq!(first_match(&counts, 5, false, &around_it), Some(At { row: 10, nth: 0 }), "every row above it is held");
    assert_eq!(first_match(&counts, 5, false, &only_it), None, "one above it may hold another");
    assert_eq!(first_match(&counts, 5, true, &only_it), Some(At { row: 10, nth: 0 }), "until every match is counted");
    assert_eq!(first_match(&counts, 60, false, &around_it), None);
    assert_eq!(
        first_match(&counts, 60, true, &around_it),
        Some(At { row: 10, nth: 0 }),
        "none below: the first of all"
    );
}

#[test]
fn the_bar_says_whether_there_may_be_more() {
    assert_eq!(says(Some(0), 3, More::No), "1 of 3");
    assert_eq!(says(Some(0), 3, More::Counting), "1 of 3…");
    assert_eq!(says(Some(0), 3, More::Unlooked), "1 of 3+");
    assert_eq!(says(None, 2, More::Unlooked), "2 found+");
    assert_eq!(says(None, 0, More::No), "No matches");
    assert_eq!(says(None, 0, More::Counting), "Finding…");
    assert_eq!(says(None, 0, More::Unlooked), "No matches loaded");
}
