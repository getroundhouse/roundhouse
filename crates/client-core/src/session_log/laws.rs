//! What has to hold wherever someone scrolls, in any order, in a session of any length.

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::pages::PAGE_EVENTS;
use super::tests::{is_rewritten, page, rewritten};
use super::*;
use protocol::prompt::Prompt;

/// A log as a view of an open session holds it once the subscription has delivered what it starts with.
fn opened(last_seq: Seq) -> SessionLog {
    let mut log = SessionLog::new(last_seq);
    for event in page(before_live(log.last()).next().seq().get()..last_seq.next().get()) {
        log.apply_event(event);
    }
    log
}

/// Fetches what the log asks for at this focus, and installs it. The fetch at `fails`, if any, fails, and
/// so does everything after it, as when the connection goes.
fn fetch(log: &mut SessionLog, focus: usize, fails: Option<usize>) {
    fetch_from(log, focus, fails, page);
}

/// What [`fetch`] does, from a hub that serves `served`.
fn fetch_from(log: &mut SessionLog, focus: usize, fails: Option<usize>, served: fn(Range<u64>) -> Vec<SessionEvent>) {
    let after_last = log.last().next();
    log.focus_on(focus..focus + 1, Some(focus));
    for (i, range) in log.pages_to_fetch().into_iter().enumerate() {
        if fails == Some(i) {
            log.fetch_failed();
            return;
        }
        log.install_page(range.start, served(range.start.seq().get()..range.end.min(after_last).seq().get()));
    }
}

fn scroll_to(log: &mut SessionLog, focus: usize) {
    fetch(log, focus, None);
}

/// Any `seq` of those numbered `numbers`.
fn seq_in(numbers: Range<u64>) -> impl Strategy<Value = Seq> {
    numbers.prop_map(Seq::new)
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    /// What's on screen is always there, and is the event its row says it is.
    #[test]
    fn the_row_in_focus_is_loaded_and_holds_its_own_event(last_seq in seq_in(1u64..30_000), focuses in vec(0usize..30_000, 1..12)) {
        let mut log = opened(last_seq);
        for focus in focuses {
            let focus = focus % log.row_count();

            scroll_to(&mut log, focus);

            let Row::Event(event) = log.row(focus) else {
                panic!("row {focus} isn't loaded");
            };
            prop_assert_eq!(Position::of_seq(event.seq), Position::of_row(focus));
        }
    }

    /// A small viewport leaves room in the cache target, wherever a long session has been scrolled.
    #[test]
    fn one_row_viewports_stay_within_the_cache_target(last_seq in seq_in(1u64..30_000), focuses in vec(0usize..30_000, 1..20)) {
        let mut log = opened(last_seq);
        for focus in focuses {
            let focus = focus % log.row_count();

            scroll_to(&mut log, focus);

            prop_assert!(log.loaded_events() <= MAX_LOADED_EVENTS, "{} loaded", log.loaded_events());
        }
    }

    /// Everything it asks for exists, and starts on a page boundary, so each page has one key.
    #[test]
    fn it_only_asks_for_whole_pages_that_exist(last_seq in seq_in(1u64..30_000), focus in 0usize..30_000) {
        let mut log = SessionLog::new(last_seq);
        log.focus_on(focus..focus + 1, Some(focus));

        for range in log.pages_to_fetch() {
            prop_assert!(range.start <= log.last(), "{range:?} is past {last_seq}");
            let rows = range.start.row().unwrap()..range.end.row().unwrap();
            prop_assert_eq!(rows.start % PAGE_EVENTS, 0);
            prop_assert_eq!(rows.len(), PAGE_EVENTS);
        }
    }

    /// A page on its way isn't asked for twice.
    #[test]
    fn a_page_on_its_way_is_not_asked_for_again(last_seq in seq_in(1u64..30_000), focus in 0usize..30_000) {
        let mut log = SessionLog::new(last_seq);
        log.focus_on(focus..focus + 1, Some(focus));
        log.pages_to_fetch();

        prop_assert_eq!(log.pages_to_fetch(), Vec::new());
    }

    /// A log whose subscription started after `before_live`, and has had every event since, loads any row a
    /// view scrolls to, so a page part-delivered live is never taken for a whole one.
    #[test]
    fn rows_from_live_events_and_from_scrolling_all_load(last_seq in seq_in(1u64..3_000), focuses in vec(0usize..3_000, 1..20)) {
        let mut log = opened(last_seq);
        for focus in focuses {
            let focus = focus % log.row_count();

            scroll_to(&mut log, focus);

            prop_assert!(matches!(log.row(focus), Row::Event(_)), "row {} isn't loaded", focus);
        }
    }

    /// However fetches fail, once retrying is allowed, as on a new connection, every row loads, so nothing
    /// asked for alongside a failed fetch is left waiting for good.
    #[test]
    fn after_failed_fetches_every_row_still_loads(last_seq in seq_in(1u64..3_000), steps in vec((0usize..3_000, 0usize..4), 1..20)) {
        let mut log = opened(last_seq);
        for (focus, fails) in steps {
            let focus = focus % log.row_count();

            fetch(&mut log, focus, Some(fails));
            log.retry_failed();
            scroll_to(&mut log, focus);

            prop_assert!(matches!(log.row(focus), Row::Event(_)), "row {} isn't loaded", focus);
        }
    }

    /// A log read again, as from a hub of another build, reaches as far and still shows every row it held, and once
    /// the newest pages have come live again, each row a view scrolls to is as the new hub has it.
    #[test]
    fn a_log_read_again_shows_what_it_held_until_each_row_comes_again(
        last_seq in seq_in(1u64..6_000),
        before in vec(0usize..6_000, 0..8),
        after in vec(0usize..6_000, 1..12),
    ) {
        let mut log = opened(last_seq);
        for focus in before {
            let focus = focus % log.row_count();
            scroll_to(&mut log, focus);
        }
        let (rows, held) = (log.row_count(), log.loaded_rows());

        log.read_again();

        prop_assert_eq!(log.row_count(), rows);
        prop_assert_eq!(log.loaded_rows(), held);
        for event in rewritten(log.received().next().seq().get()..last_seq.next().get()) {
            log.apply_event(event);
        }
        for focus in after {
            let focus = focus % log.row_count();

            fetch_from(&mut log, focus, None, rewritten);

            let Row::Event(event) = log.row(focus) else {
                panic!("row {focus} isn't loaded");
            };
            prop_assert!(is_rewritten(event), "row {} is as the hub before had it", focus);
            prop_assert!(log.loaded_events() <= MAX_LOADED_EVENTS, "{} loaded", log.loaded_events());
        }
    }

    /// The runs of loaded rows name every row there's something to show of, and no other, in order, wherever the
    /// session has been scrolled and with prompts sent that aren't recorded yet, which the tail shows.
    #[test]
    fn the_loaded_rows_are_the_ones_that_hold_something(
        last_seq in seq_in(1u64..6_000),
        focuses in vec(0usize..6_000, 0..12),
        sent in 0usize..3,
    ) {
        let mut log = opened(last_seq);
        for focus in focuses {
            let focus = focus % log.row_count();
            scroll_to(&mut log, focus);
        }
        for prompt in 0..sent {
            log.add_pending(QueuedPrompt { client_msg_id: format!("m{prompt}").into(), prompt: Prompt::new("hello") });
        }

        let runs = log.loaded_rows();

        prop_assert!(runs.iter().all(|run| !run.is_empty()), "{:?}", runs);
        prop_assert!(runs.windows(2).all(|pair| pair[0].end < pair[1].start), "not in order, or not joined: {:?}", runs);
        let mut loaded = vec![false; log.row_count()];
        for row in runs.iter().flat_map(Clone::clone) {
            prop_assert!(row < loaded.len(), "row {} is past the end", row);
            loaded[row] = true;
        }
        for (row, loaded) in loaded.into_iter().enumerate() {
            prop_assert_eq!(loaded, log.row(row) != Row::Unloaded, "row {}", row);
        }
    }
}
