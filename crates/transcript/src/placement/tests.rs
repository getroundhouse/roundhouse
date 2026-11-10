use std::ops::Range;

use client_core::session_log::SessionLog;
use protocol::event::{ContentBlock, SessionEvent};
use protocol::model::{Seq, ToolUseId};

use super::{LogRow, Placement, Position, Splice, Update, Work};
use crate::calls::agent;
use crate::fixtures::{calls, calls_to, event, log, result, text, within};
use crate::folds::Folds;
use crate::item_key::ItemKey;

/// The log's rows in the order they're drawn.
pub(super) fn drawn(placement: &Placement) -> Vec<usize> {
    (0..placement.len()).filter_map(|position| placement.row_at(Position(position))).map(|row| row.0).collect()
}

fn position_of(placement: &Placement, row: usize) -> Option<usize> {
    placement.position_of(LogRow(row)).map(|position| position.0)
}

fn drawn_with(placement: &Placement, row: usize) -> Option<Range<usize>> {
    placement.drawn_with(LogRow(row)).map(|positions| positions.start.0..positions.end.0)
}

/// The call `row` is drawn after, as a subagent's work.
fn under(placement: &Placement, row: usize) -> Option<&str> {
    placement.within(LogRow(row)).map(ToolUseId::as_str)
}

/// The positions measured again, though they stay.
fn restyled(update: &Update) -> Vec<usize> {
    update.restyled.iter().flat_map(|run| run.start.0..run.end.0).collect()
}

fn work(placement: &Placement, id: &str) -> Option<Work> {
    placement.call(&ToolUseId::from(id)).map(|call| call.work.clone())
}

fn added(at: usize, rows: usize) -> Splice {
    Splice { at: Position(at), removed: 0, added: rows }
}

fn removed(at: usize, rows: usize) -> Splice {
    Splice { at: Position(at), removed: rows, added: 0 }
}

fn agent_call(seq: Seq, id: &str) -> SessionEvent {
    event(seq, calls_to("Agent", &[id]))
}

/// Opens every call in the log that started a subagent, as clicking each would.
fn open_every_call(log: &SessionLog, folds: &mut Folds) {
    for event in log.loaded() {
        for (position, block) in event.body.blocks().iter().enumerate() {
            if matches!(block, ContentBlock::ToolUse { name, .. } if agent::hands_off(name)) {
                folds.choose(ItemKey::of_block(event.seq, &event.body, position), true);
            }
        }
    }
}

/// Where the log's rows go with every call open.
fn opened(log: &SessionLog) -> Placement {
    let mut folds = Folds::default();
    open_every_call(log, &mut folds);
    Placement::of(log, &folds)
}

#[test]
fn a_subagent_s_work_follows_its_call_ahead_of_what_came_between() {
    let events = [
        calls_to("Agent", &["agent"]),
        within("agent", calls(&["a"])),
        calls(&["the agent's own"]),
        within("another", calls(&["b"])),
        within("agent", text("found it")),
        within("agent", calls(&["c"])),
    ];
    let log = log((1..).zip(events).map(|(seq, body)| event(seq, body)));

    let placement = opened(&log);

    assert_eq!(drawn(&placement), [0, 1, 4, 5, 2, 3], "work with no call it can find stays where it arrived");
    assert_eq!(work(&placement, "agent"), Some(Work { calls: 2, said: true, last: Some(LogRow(5)) }));
    assert_eq!(under(&placement, 4), Some("agent"));
    assert_eq!(under(&placement, 2), None);
    assert_eq!(position_of(&placement, 2), Some(4));
    assert_eq!(drawn_with(&placement, 0), Some(0..4), "the call, and its work under it");
    assert_eq!(drawn_with(&placement, 2), Some(4..5));
}

#[test]
fn a_closed_call_s_work_is_left_out_until_it_opens() {
    let events = [calls_to("Agent", &["agent"]), within("agent", calls(&["a"])), calls(&["own"])];
    let log = log((1..).zip(events).map(|(seq, body)| event(seq, body)));
    let mut folds = Folds::default();
    let mut placement = Placement::of(&log, &folds);
    assert_eq!(drawn(&placement), [0, 2], "calls start closed");
    assert_eq!(position_of(&placement, 1), None);
    assert_eq!(drawn_with(&placement, 1), None);
    assert_eq!(work(&placement, "agent"), Some(Work { calls: 1, said: false, last: None }), "its line counts the work");
    assert_eq!(drawn_with(&placement, 0), Some(0..1));

    open_every_call(&log, &mut folds);
    let update = placement.follow(&log, &folds);

    assert_eq!(drawn(&placement), [0, 1, 2]);
    assert_eq!(update.splices, [added(1, 1)], "only its work goes in");
    assert_eq!(restyled(&update), [0, 1], "the call, which no longer ends its rule, and the work that does");
    assert_eq!(drawn_with(&placement, 0), Some(0..2));
}

#[test]
fn a_subagent_s_event_out_of_its_call_s_reach_stays_where_it_arrived() {
    let report = |seq| event(seq, within("agent", calls(&["report"])));
    let log = log([agent_call(1, "agent"), report(2), report(2001), report(2002)]);

    let placement = opened(&log);

    assert_eq!(under(&placement, 2000), Some("agent"), "the last event the call gathers");
    assert_eq!(under(&placement, 2001), None, "too far for the call to gather");
    assert_eq!(position_of(&placement, 2000), Some(2));
    assert_eq!(position_of(&placement, 2001), Some(2001));
    assert_eq!(placement.len(), 2002);
}

#[test]
fn a_call_loaded_late_gathers_work_that_arrived_before_it() {
    let mut log = log([event(5, within("agent", calls(&["a"])))]);
    let mut folds = Folds::default();
    let mut placement = Placement::of(&log, &folds);
    assert_eq!(under(&placement, 4), None, "its call isn't loaded");

    log.apply_event(agent_call(1, "agent"));
    open_every_call(&log, &mut folds);
    let update = placement.follow(&log, &folds);

    assert_eq!(under(&placement, 4), Some("agent"));
    assert_eq!(drawn(&placement), [0, 4, 1, 2, 3]);
    assert_eq!(update.splices, [removed(4, 1), added(1, 1)]);
}

#[test]
fn work_whose_call_the_log_lets_go_of_stays_where_it_arrived() {
    let work = |seq| event(seq, within("agent", calls(&["work"])));
    let mut log = log((1..=1500).map(|seq| event(seq, result("x"))));
    log.apply_event(agent_call(1501, "agent"));
    log.apply_event(work(1502));
    let mut folds = Folds::default();
    open_every_call(&log, &mut folds);
    let mut placement = Placement::of(&log, &folds);
    assert_eq!(under(&placement, 1501), Some("agent"));

    for seq in 1503..=2500 {
        log.apply_event(event(seq, result("x")));
    }
    log.apply_event(work(2501));
    let update = placement.follow(&log, &folds);

    assert_eq!(under(&placement, 2500), None, "drawn where it is, since its call isn't loaded");
    assert_eq!(position_of(&placement, 2500), Some(2500));
    assert!(placement.call(&ToolUseId::from("agent")).is_none());
    assert!(restyled(&update).contains(&1500), "the call ends its own rule again");
}

#[test]
fn only_a_call_that_hands_off_gathers_work() {
    let log = log([event(1, calls(&["not-an-agent"])), event(2, within("not-an-agent", calls(&["a"])))]);

    let placement = opened(&log);

    assert_eq!(under(&placement, 1), None);
    assert_eq!(drawn(&placement), [0, 1]);
}

#[test]
fn a_subagent_s_own_subagent_s_work_follows_its_call_inside_the_first() {
    let events = [
        calls_to("Agent", &["outer"]),
        within("outer", calls_to("Agent", &["inner"])),
        calls(&["the agent's own"]),
        within("inner", calls(&["deep"])),
        within("outer", text("done")),
    ];
    let log = log((1..).zip(events).map(|(seq, body)| event(seq, body)));

    let placement = opened(&log);

    assert_eq!(drawn(&placement), [0, 1, 3, 4, 2]);
    assert_eq!(
        work(&placement, "outer"),
        Some(Work { calls: 1, said: true, last: Some(LogRow(4)) }),
        "it counts its own calls"
    );
    assert_eq!(work(&placement, "inner"), Some(Work { calls: 1, said: false, last: Some(LogRow(3)) }));
    assert_eq!(drawn_with(&placement, 0), Some(0..4));
    assert_eq!(drawn_with(&placement, 1), Some(1..3));
}

#[test]
fn a_row_with_two_calls_has_each_one_s_work_after_it_in_turn() {
    let events =
        [calls_to("Agent", &["first", "second"]), within("second", calls(&["b"])), within("first", calls(&["a"]))];
    let log = log((1..).zip(events).map(|(seq, body)| event(seq, body)));

    let placement = opened(&log);

    assert_eq!(drawn(&placement), [0, 2, 1]);
    assert_eq!(drawn_with(&placement, 0), Some(0..3));
    let key = |id| placement.call(&ToolUseId::from(id)).map(|call| call.key.clone());
    assert_ne!(key("first"), key("second"), "each opens on its own");
}

#[test]
fn work_that_arrives_goes_in_after_the_rest_of_its_call_s() {
    let mut log = log([agent_call(1, "agent"), event(2, within("agent", calls(&["a"]))), event(3, calls(&["own"]))]);
    let mut folds = Folds::default();
    open_every_call(&log, &mut folds);
    let mut placement = Placement::of(&log, &folds);

    log.apply_event(event(4, within("agent", calls(&["b"]))));
    let update = placement.follow(&log, &folds);

    assert_eq!(drawn(&placement), [0, 1, 3, 2]);
    assert_eq!(update.splices, [added(2, 1)], "only the new row, so nothing else is measured from scratch");
    assert_eq!(restyled(&update), [0, 1, 2], "the call, and the rows that end its rule before and after");
    assert_eq!(placement.follow(&log, &folds), Update::default(), "nothing more until the log changes");
}

#[test]
fn a_log_with_no_subagents_is_drawn_in_order() {
    let mut log = log([event(1, text("a")), event(2, calls(&["x"])), event(3, result("x"))]);
    let folds = Folds::default();
    let mut placement = Placement::of(&log, &folds);

    log.apply_event(event(4, text("b")));
    let update = placement.follow(&log, &folds);

    assert_eq!(drawn(&placement), [0, 1, 2, 3]);
    assert_eq!(update, Update { splices: vec![added(3, 1)], restyled: Vec::new() });
    assert_eq!(placement.row_at(Position(4)), None, "the streaming tail");
    assert_eq!(placement.tail(), Position(4));
    assert_eq!(position_of(&placement, 4), None);
}
