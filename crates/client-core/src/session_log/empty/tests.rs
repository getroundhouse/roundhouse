use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::Empty;

#[test]
fn consecutive_empty_events_keep_only_two_boundaries_in_either_order() {
    for backwards in [false, true] {
        let mut empty = Empty::default();
        for row in 0..10_000 {
            empty.insert(if backwards { 9999 - row } else { row });
        }
        assert_eq!(empty.ranges().collect::<Vec<_>>(), vec![0..10_000]);
        assert_eq!(empty.count(400..650), 250);
    }
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    #[test]
    fn empty_runs_agree_with_each_row_after_inserts_and_rewrites(
        changes in proptest::collection::vec((0usize..80, any::<bool>()), 0..150),
        start in 0usize..100,
        len in 0usize..100,
    ) {
        let mut empty = Empty::default();
        let mut expected = [false; 200];
        for (row, inserted) in changes {
            expected[row] = inserted;
            if inserted { empty.insert(row); } else { empty.remove(row); }
        }

        for (row, present) in expected.iter().enumerate() {
            prop_assert_eq!(empty.contains(row), *present);
        }
        let runs = empty.ranges().collect::<Vec<_>>();
        prop_assert!(runs.iter().all(|run| !run.is_empty()));
        prop_assert!(runs.windows(2).all(|pair| pair[0].end < pair[1].start));
        prop_assert_eq!(empty.count(start..start + len), expected[start..start + len].iter().filter(|set| **set).count());
    }
}
