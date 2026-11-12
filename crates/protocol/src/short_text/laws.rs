//! What has to hold for a line or a start made from any text, at any limit.

use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::*;

/// Text that often has what a line must not: line breaks of every kind, controls, and characters that hide or reorder
/// text, between ordinary words and emoji.
pub(crate) fn text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        "[a-zé ]{0,12}",
        Just("\n".to_string()),
        Just("\r".to_string()),
        Just("\u{2028}".to_string()),
        Just("\u{85}".to_string()),
        Just("\t".to_string()),
        Just("\u{1b}[2J".to_string()),
        Just("\0".to_string()),
        Just("\u{200B}".to_string()),
        Just("\u{202E}".to_string()),
        Just("\u{2066}".to_string()),
        Just("\u{FEFF}".to_string()),
        Just("👩\u{200D}💻".to_string()),
        "\\PC{0,4}",
    ];
    proptest::collection::vec(piece, 0..12).prop_map(|pieces| pieces.concat())
}

/// What every line holds to, however it was made: single spaces between words, none at either end, and nothing that
/// breaks the line, hides or reorders text.
pub(crate) fn reads_on_one_line(text: &str) {
    assert_eq!(text, text.trim(), "{text:?}");
    assert!(!text.contains("  "), "{text:?}");
    assert!(!text.chars().any(|c| c.is_control() || hides(c) || (c != ' ' && c.is_whitespace())), "{text:?}");
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    /// A limit only cuts the line short, as late as it can, and never leaves a space at the cut.
    #[test]
    fn a_line_with_a_limit_is_the_longest_start_of_the_whole_line_that_fits(text in text(), most in 0..80usize) {
        let whole = one_line(&text, usize::MAX);
        let line = one_line(&text, most);

        reads_on_one_line(&line);
        let chars = line.chars().count();
        prop_assert!(chars <= most, "{line:?} is longer than {most}");
        prop_assert!(whole.starts_with(&line), "{line:?} doesn't start {whole:?}");
        if line != whole {
            let rest = &whole[line.len()..];
            let next = if rest.starts_with(' ') { 2 } else { 1 };
            prop_assert!(chars + next > most, "{line:?} stopped before {rest:?} with room left");
        }
    }

    /// A line made again from itself is the same, so a line read back from storage or the wire reads as it was made.
    #[test]
    fn a_line_made_again_is_the_same(text in text(), most in 0..80usize) {
        let line = one_line(&text, most);

        prop_assert_eq!(one_line(&line, most), line);
    }

    /// The start of a text is in whole characters, no more than `most` of them, and cut exactly when the text had more.
    #[test]
    fn the_start_of_a_text_says_whether_it_was_cut(text in "\\PC{0,40}", most in 0usize..30) {
        let (start, cut) = start_of(&text, most);

        prop_assert!(text.starts_with(start));
        prop_assert_eq!(start.chars().count(), text.chars().count().min(most));
        prop_assert_eq!(cut, text.chars().count() > most);
    }
}
