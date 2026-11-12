//! What has to hold however a view asks for a file's lines, and however much of what's asked comes back.

use std::collections::BTreeSet;
use std::ops::Range;

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use protocol::model::{FilePage, OpenedText, PageLine};

use super::FileText;

/// A file of `lines` lines, opened with nothing sent, whose line `n` says `n`.
fn opened(lines: u32, page_lines: u32) -> FileText {
    FileText::new(OpenedText { bytes: 0, lines, page_lines, pages: Vec::new() })
}

/// Page `index` as the hub sends it.
fn page(index: u32, lines: u32, page_lines: u32) -> FilePage {
    let first = index * page_lines + 1;
    let lines = (first..(first + page_lines).min(lines + 1)).map(|n| PageLine { text: n.to_string(), cut: None });
    FilePage { index, lines: lines.collect() }
}

/// What a view asks for, and which of the pages asked for come back.
#[derive(Clone, Debug)]
struct Step {
    wanted: Vec<Range<u32>>,
    /// Whether each page asked for comes, by its place among them, round and round.
    comes: Vec<bool>,
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    let range = (0_u32..45, 0_u32..12).prop_map(|(start, count)| start..start + count);
    let step = (vec(range, 0..4), vec(any::<bool>(), 1..4)).prop_map(|(wanted, comes)| Step { wanted, comes });
    vec(step, 1..8)
}

/// One run of lines, by number, as a view asks for them.
fn run(lines: Range<u32>) -> [Range<u32>; 1] {
    [lines]
}

#[test]
fn a_page_being_fetched_isnt_asked_for_again_until_it_comes_or_doesnt() {
    let mut text = opened(10, 4);

    let first = text.to_fetch(&run(2..6));
    let again = text.to_fetch(&run(5..10));
    text.fetched(&first, vec![page(0, 10, 4)]);

    assert_eq!((first, again), (vec![0, 1], vec![2]));
    let after = text.to_fetch(&run(1..12));
    assert_eq!(after, [1], "the page that didn't come, but not the one still being fetched, nor one past the end");
    assert_eq!(text.held(), run(1..5));
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// Each page is asked for once until it comes or doesn't, only pages in the file are asked for, and what it holds
    /// is the file's own lines, by their numbers.
    #[test]
    fn it_holds_the_file_s_own_lines_and_asks_for_each_page_once(
        lines in 0_u32..40,
        page_lines in 1_u32..5,
        steps in steps(),
    ) {
        let mut text = opened(lines, page_lines);
        let mut held = BTreeSet::new();
        for Step { wanted, comes } in steps {
            let asked = text.to_fetch(&wanted);

            prop_assert!(asked.iter().all(|index| !held.contains(index) && index * page_lines < lines), "{asked:?}");
            prop_assert!(text.to_fetch(&wanted).is_empty(), "asked for again while being fetched");
            let came: Vec<u32> = asked.iter().zip(comes.iter().cycle()).filter(|(_, c)| **c).map(|(i, _)| *i).collect();
            let taken = text.fetched(&asked, came.iter().map(|index| page(*index, lines, page_lines)).collect());
            prop_assert_eq!(&taken, &came);
            held.extend(came);
            for number in wanted.iter().flat_map(Clone::clone).filter(|number| (1..=lines).contains(number)) {
                let expected = held.contains(&((number - 1) / page_lines)).then(|| number.to_string());
                prop_assert_eq!(text.line(number).map(|line| line.text.clone()), expected);
            }
            let holding: Vec<u32> = text.held().into_iter().flatten().collect();
            prop_assert_eq!(holding, (1..=lines).filter(|n| text.line(*n).is_some()).collect::<Vec<_>>());
        }
    }
}
