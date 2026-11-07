//! What has to hold wherever someone scrolls, in any order, in a session of any length.

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::tests::page;
use super::*;

/// Fetches what the log asks for at this focus, and installs it.
fn scroll_to(log: &mut SessionLog, focus: usize) {
    let last_seq = log.last_seq();
    for range in log.pages_to_fetch(focus) {
        let events = page(range.start..range.end.min(last_seq + 1));
        log.install_page(range.start, events, focus);
    }
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
        let mut log = SessionLog::new(last_seq);
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
        let mut log = SessionLog::new(last_seq);
        for focus in focuses {
            scroll_to(&mut log, focus % seq_index(last_seq));

            prop_assert!(log.loaded_events() <= MAX_LOADED_EVENTS, "{} loaded", log.loaded_events());
        }
    }

    /// Everything it asks for exists, and starts on a page boundary, so each page has one key.
    #[test]
    fn it_only_asks_for_whole_pages_that_exist(last_seq in 1u64..30_000, focus in 0usize..30_000) {
        let mut log = SessionLog::new(last_seq);

        for range in log.pages_to_fetch(focus) {
            prop_assert!(range.start <= last_seq, "{range:?} is past {last_seq}");
            prop_assert_eq!((range.start - 1) % PAGE_EVENTS, 0);
            prop_assert_eq!(range.end - range.start, PAGE_EVENTS);
        }
    }

    /// A page on its way isn't asked for twice.
    #[test]
    fn a_page_on_its_way_is_not_asked_for_again(last_seq in 1u64..30_000, focus in 0usize..30_000) {
        let mut log = SessionLog::new(last_seq);
        log.pages_to_fetch(focus);

        prop_assert_eq!(log.pages_to_fetch(focus), Vec::new());
    }
}
