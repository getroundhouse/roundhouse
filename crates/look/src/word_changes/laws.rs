//! What has to hold for any run of lines, however it changed.

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::*;

/// Lines of a few words, with the punctuation, space, comment marks and wide characters that make diffs awkward.
fn lines() -> impl Strategy<Value = Vec<String>> {
    vec("[ab_ .(:\t#é]{0,8}", 0..4)
}

/// What isn't marked of `lines`, after the start every line shares, without space.
fn unmarked(lines: &[&str], marks: &[Vec<Range<usize>>], start: usize) -> String {
    let mut kept = String::new();
    for (line, ranges) in lines.iter().zip(marks) {
        let mut at = start.min(line.len());
        for range in ranges {
            kept.extend(line[at..range.start.max(at)].chars().filter(|c| !c.is_whitespace()));
            at = at.max(range.end);
        }
        kept.extend(line[at..].chars().filter(|c| !c.is_whitespace()));
    }
    kept
}

proptest! {
    #![proptest_config(Config { cases: 2048, rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    #[test]
    fn marks_are_in_order_apart_and_on_character_boundaries(removed in lines(), added in lines()) {
        let (removed, added): (Vec<&str>, Vec<&str>) = (removed.iter().map(String::as_str).collect(), added.iter().map(String::as_str).collect());

        let (lost, gained) = compared(&removed, &added);

        for (line, ranges) in removed.iter().zip(&lost).chain(added.iter().zip(&gained)) {
            for pair in ranges.windows(2) {
                prop_assert!(pair[0].end < pair[1].start, "{ranges:?} of {line:?}");
            }
            for range in ranges {
                prop_assert!(range.start < range.end && line.get(range.clone()).is_some(), "{range:?} of {line:?}");
            }
        }
    }

    #[test]
    fn what_isn_t_marked_reads_the_same_on_both_sides(removed in lines(), added in lines()) {
        let (removed, added): (Vec<&str>, Vec<&str>) = (removed.iter().map(String::as_str).collect(), added.iter().map(String::as_str).collect());

        let (lost, gained) = compared(&removed, &added);

        let start = shared_start(&[&removed[..], &added[..]].concat());
        if lost.iter().chain(&gained).any(|ranges| !ranges.is_empty()) {
            prop_assert_eq!(unmarked(&removed, &lost, start), unmarked(&added, &gained, start));
        }
    }
}
