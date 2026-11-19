//! A viewport can cover more event positions than the cache target, particularly through hidden events.

use std::collections::BTreeSet;
use std::ops::Range;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use protocol::event::{AgentRunState, EventBody, SessionEvent};
use protocol::fixtures::event;
use protocol::model::{Place, Seq};
use protocol::prompt::Prompt;

use super::tests::page;
use super::{By, MAX_LOADED_EVENTS, Position, PromptState, Row, SessionLog};
use protocol::prompt::PromptFrom;

fn fetch(log: &mut SessionLog, make: impl Fn(Range<u64>) -> Vec<SessionEvent>) -> Vec<Range<Position>> {
    let mut fetched = Vec::new();
    let mut starts = BTreeSet::new();
    loop {
        // Each completed batch wakes the view, which reports its current range and lowest undrawn row again.
        let rows = log.focus.clone();
        let unfetched = rows.clone().rev().find(|row| log.row(*row) == Row::Unloaded);
        log.focus_on(rows, unfetched);
        let ranges = log.pages_to_fetch();
        if ranges.is_empty() {
            return fetched;
        }
        assert!(ranges.len() <= 3, "yield to live output between batches");
        for range in &ranges {
            assert!(starts.insert(range.start), "a stationary viewport fetched the same page again");
            log.install_page(range.start, make(range.start.seq().get()..range.end.seq().get()));
        }
        fetched.extend(ranges);
    }
}

#[test]
fn the_entire_viewport_survives_loading_and_live_updates_then_can_be_evicted() {
    let mut log = SessionLog::new(Seq::new(20_000));
    let visible = 2000..7000;
    log.focus_on(visible.clone(), Some(visible.start));
    let fetched = fetch(&mut log, page);

    assert!(log.holds(visible.clone()), "loading its bottom must not evict its top");
    assert!(log.pages_to_fetch().is_empty(), "a stationary viewport must stop fetching");
    assert_eq!(fetched.iter().map(|range| range.start).collect::<BTreeSet<_>>().len(), fetched.len());
    for event in page(20_001..24_001) {
        log.apply_event(event);
        assert!(log.holds(visible.clone()), "live output must not evict the viewport");
    }

    log.focus_on(12_000..12_020, Some(12_000));
    fetch(&mut log, page);
    assert_eq!(log.row(2000), Row::Unloaded, "scrolling away releases the old working set");
    assert!(log.loaded_events() <= MAX_LOADED_EVENTS);
}

/// Like a saved Codex transcript: visible rows separated by hundreds of unsupported notifications.
fn sparse(range: Range<u64>) -> Vec<SessionEvent> {
    range
        .map(|seq| {
            let body = if seq % 600 == 0 {
                EventBody::AgentState { state: AgentRunState::Running }
            } else {
                EventBody::AgentOther { kind: "item/commandExecution/outputDelta".into(), subagent: None }
            };
            SessionEvent { place: Place::new(seq), ..event(Seq::new(seq), body) }
        })
        .collect()
}

#[test]
fn sparse_history_stays_loaded_across_the_viewport_and_releases_when_scrolled_away() {
    for by in [By::Seq, By::Place] {
        let mut log = match by {
            By::Seq => SessionLog::new(Seq::new(20_000)),
            By::Place => SessionLog::by_place(Place::new(20_000)),
        };
        let served = |range| {
            let mut events = sparse(range);
            if by == By::Place {
                for event in &mut events {
                    event.seq = Seq::new(event.seq.get() * 3);
                }
            }
            events
        };
        log.focus_on(1000..9000, Some(1000));
        fetch(&mut log, served);

        assert!(log.holds(1000..9000));
        assert!(log.pages_to_fetch().is_empty());
        let expected_seq = Seq::new(if by == By::Place { 3600 } else { 1200 });
        assert!(matches!(log.row(1199), Row::Event(event) if event.seq == expected_seq));
        assert!(matches!(log.row(1200), Row::Event(event) if matches!(event.body, EventBody::AgentOther { .. })));
        assert_eq!(log.row(0), Row::Unloaded);

        for row in 20_001..24_001 {
            log.apply_event(served(row..row + 1).pop().unwrap());
        }
        assert!(log.holds(1000..9000));
        assert!(log.pages_to_fetch().is_empty());

        log.focus_on(15_000..15_020, Some(15_000));
        fetch(&mut log, served);
        assert_eq!(log.row(1200), Row::Unloaded, "scrolling away releases sparse pages too");
        assert!(log.loaded_events() <= MAX_LOADED_EVENTS);
    }
}

#[test]
fn a_hub_upgrade_rereads_stale_pages_while_keeping_snapshots_immutable() {
    let mut log = SessionLog::new(Seq::new(20_000));
    log.focus_on(0..500, Some(0));
    let other = |range: Range<u64>| {
        range
            .map(|seq| event(Seq::new(seq), EventBody::AgentOther { kind: "future/event".into(), subagent: None }))
            .collect()
    };
    fetch(&mut log, other);
    assert_eq!(log.loaded_rows(), vec![0..750]);
    assert!(log.pages_to_fetch().is_empty());
    let before = log.clone();

    log.read_again();
    assert!(log.wants_pages(), "a migration may have rewritten the loaded pages");
    assert!(
        matches!(log.row(1), Row::Event(event) if matches!(event.body, EventBody::AgentOther { .. })),
        "loaded events stay in place until replaced"
    );
    fetch(&mut log, page);
    assert!(matches!(log.row(1), Row::Event(event) if matches!(event.body, EventBody::Notice { .. })));
    assert!(
        matches!(before.row(1), Row::Event(event) if matches!(event.body, EventBody::AgentOther { .. })),
        "the previous frame's snapshot stays immutable"
    );
}

#[test]
fn an_agent_other_event_does_not_make_an_unsettled_prompt_look_taken() {
    let mut log = SessionLog::default();
    log.apply_event(event(
        Seq::FIRST,
        EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("waiting"), from: PromptFrom::Person },
    ));
    log.apply_event(event(Seq::new(2), EventBody::AgentOther { kind: "future/event".into(), subagent: None }));

    assert_eq!(log.prompt_state(Seq::FIRST), PromptState::Waiting);
}

#[test]
fn a_loaded_viewport_does_not_prefetch_unneeded_neighbouring_pages() {
    let mut log = SessionLog::new(Seq::new(20_000));
    log.install_page(Position::of_row(1000), page(1001..1251));
    log.focus_on(1000..1100, None);

    assert!(!log.wants_pages(), "reporting loaded rows must not fetch more history");
    assert!(log.pages_to_fetch().is_empty());
    assert_eq!(log.row(999), Row::Unloaded);

    log.focus_on(990..1100, Some(990));
    fetch(&mut log, page);
    assert!(log.holds(990..1100), "scrolling into missing history fetches it");
    log.read_again();
    assert!(log.wants_pages(), "a new hub must reread stale bodies even while they are drawn");
    fetch(&mut log, page);
    assert!(!log.wants_pages());
}

#[test]
fn a_page_reply_with_no_events_is_not_fetched_in_a_loop() {
    let mut log = SessionLog::new(Seq::new(1000));
    log.focus_on(0..1, Some(0));
    let asked = log.pages_to_fetch();
    assert!(!asked.is_empty());
    for range in asked {
        log.install_page(range.start, Vec::new());
    }

    assert!(!log.wants_pages(), "a completed reply stays completed, even if the hub had no events there");
    log.read_again();
    assert!(log.wants_pages(), "a new hub build can still replace that reply");
}

proptest! {
    #![proptest_config(Config {
        cases: 32, rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default()
    })]

    #[test]
    fn every_row_in_each_viewport_is_held_after_paging_settles(
        views in proptest::collection::vec((0usize..20_000, 1usize..5000), 1..5),
    ) {
        let mut log = SessionLog::new(Seq::new(30_000));
        for (start, len) in views {
            let rows = start..start + len;
            log.focus_on(rows.clone(), Some(rows.start));
            fetch(&mut log, page);

            prop_assert!(log.holds(rows));
            prop_assert!(!log.wants_pages());
        }
    }
}
