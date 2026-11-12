//! What has to hold wherever someone scrolls, in any order, in a session of any length.

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::tests::page;
use super::*;
use protocol::prompt::Prompt;

/// A log as a view of an open session holds it once the subscription has delivered what it starts with.
fn opened(last_seq: Seq) -> SessionLog {
    let mut log = SessionLog::new(last_seq);
    for event in page(first_live(last_seq)..last_seq + 1) {
        log.apply_event(event);
    }
    log
}

/// Fetches what the log asks for at this focus, and installs it. The fetch at `fails`, if any, fails, and
/// so does everything after it, as when the connection goes.
fn fetch(log: &mut SessionLog, focus: usize, fails: Option<usize>) {
    let last_seq = log.last();
    log.focus_on(focus);
    for (i, range) in log.pages_to_fetch().into_iter().enumerate() {
        if fails == Some(i) {
            log.fetch_failed();
            return;
        }
        log.install_page(range.start, page(range.start..range.end.min(last_seq + 1)));
    }
}

fn scroll_to(log: &mut SessionLog, focus: usize) {
    fetch(log, focus, None);
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    /// What's on screen is always there, and is the event its row says it is.
    #[test]
    fn the_row_in_focus_is_loaded_and_holds_its_own_event(last_seq in 1u64..30_000, focuses in vec(0usize..30_000, 1..40)) {
        let mut log = opened(last_seq);
        for focus in focuses {
            let focus = focus % seq_index(last_seq);

            scroll_to(&mut log, focus);

            let Row::Event(event) = log.row(focus) else {
                panic!("row {focus} isn't loaded");
            };
            prop_assert_eq!(seq_index(event.seq), focus + 1);
        }
    }

    /// A long session never ends up all in memory, wherever it's been scrolled.
    #[test]
    fn it_never_holds_more_than_its_limit(last_seq in 1u64..30_000, focuses in vec(0usize..30_000, 1..80)) {
        let mut log = opened(last_seq);
        for focus in focuses {
            scroll_to(&mut log, focus % seq_index(last_seq));

            prop_assert!(log.loaded_events() <= MAX_LOADED_EVENTS, "{} loaded", log.loaded_events());
        }
    }

    /// Everything it asks for exists, and starts on a page boundary, so each page has one key.
    #[test]
    fn it_only_asks_for_whole_pages_that_exist(last_seq in 1u64..30_000, focus in 0usize..30_000) {
        let mut log = SessionLog::new(last_seq);
        log.focus_on(focus);

        for range in log.pages_to_fetch() {
            prop_assert!(range.start <= last_seq, "{range:?} is past {last_seq}");
            prop_assert_eq!((range.start - 1) % PAGE_EVENTS, 0);
            prop_assert_eq!(range.end - range.start, PAGE_EVENTS);
        }
    }

    /// A page on its way isn't asked for twice.
    #[test]
    fn a_page_on_its_way_is_not_asked_for_again(last_seq in 1u64..30_000, focus in 0usize..30_000) {
        let mut log = SessionLog::new(last_seq);
        log.focus_on(focus);
        log.pages_to_fetch();

        prop_assert_eq!(log.pages_to_fetch(), Vec::new());
    }

    /// A log whose subscription started at `first_live`, and has had every event since, loads any row a
    /// view scrolls to, so a page part-delivered live is never taken for a whole one.
    #[test]
    fn rows_from_live_events_and_from_scrolling_all_load(last_seq in 1u64..3_000, focuses in vec(0usize..3_000, 1..20)) {
        let mut log = opened(last_seq);
        for focus in focuses {
            let focus = focus % seq_index(last_seq);

            scroll_to(&mut log, focus);

            prop_assert!(matches!(log.row(focus), Row::Event(_)), "row {} isn't loaded", focus);
        }
    }

    /// However fetches fail, once retrying is allowed, as on a new connection, every row loads, so nothing
    /// asked for alongside a failed fetch is left waiting for good.
    #[test]
    fn after_failed_fetches_every_row_still_loads(last_seq in 1u64..3_000, steps in vec((0usize..3_000, 0usize..4), 1..20)) {
        let mut log = opened(last_seq);
        for (focus, fails) in steps {
            let focus = focus % seq_index(last_seq);

            fetch(&mut log, focus, Some(fails));
            log.retry_failed();
            scroll_to(&mut log, focus);

            prop_assert!(matches!(log.row(focus), Row::Event(_)), "row {} isn't loaded", focus);
        }
    }

    /// The runs of loaded rows name every row there's something to show of, and no other, in order, wherever the
    /// session has been scrolled and with prompts sent that aren't recorded yet, which the tail shows.
    #[test]
    fn the_loaded_rows_are_the_ones_that_hold_something(
        last_seq in 1u64..6_000,
        focuses in vec(0usize..6_000, 0..12),
        sent in 0usize..3,
    ) {
        let mut log = opened(last_seq);
        for focus in focuses {
            scroll_to(&mut log, focus % seq_index(last_seq));
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
