//! What has to hold for a title made from any text.

use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::*;
use crate::short_text::laws::{reads_on_one_line, text};

/// What every title holds to, however it was made.
fn holds(title: &Title) {
    assert!(!title.is_empty(), "{title:?}");
    reads_on_one_line(title);
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    #[test]
    fn every_way_to_make_a_title_tidies_it(text in text()) {
        for title in [Title::tidied(&text), Title::from_first_line(&text), Title::typed(&text).ok()].iter().flatten() {
            holds(title);
        }
    }

    /// Typing refuses only what tidying leaves empty or too long, and otherwise keeps every word.
    #[test]
    fn a_typed_title_is_its_text_tidied(text in text()) {
        match (Title::typed(&text), Title::tidied(&text)) {
            (Ok(typed), Some(tidied)) => prop_assert_eq!(typed, tidied),
            (Err(TitleError::Empty), None) => {}
            (Err(TitleError::TooLong { most }), Some(tidied)) => prop_assert!(tidied.chars().count() > most),
            other => prop_assert!(false, "{other:?}"),
        }
    }

    /// A first line is short, never spans a line break, and has a title whenever the text has a word.
    #[test]
    fn a_first_line_is_one_short_line(text in text()) {
        let first = Title::from_first_line(&text);
        prop_assert_eq!(first.is_some(), Title::tidied(&text).is_some());
        if let Some(first) = first {
            prop_assert!(first.chars().count() <= 49, "{first:?}");
            let line = text.split(breaks_line).find_map(Title::tidied).unwrap();
            let kept = first.trim_suffix('…');
            prop_assert!(line.starts_with(kept), "{first:?} from {line:?}");
        }
    }

    /// Reading a title back tidies nothing more, so a stored title reads as it was saved.
    #[test]
    fn a_tidied_title_reads_back_the_same(text in text()) {
        if let Some(title) = Title::tidied(&text) {
            prop_assert_eq!(Title::tidied(&title), Some(title.clone()));
        }
    }
}

#[test]
fn a_first_line_is_cut_at_a_word() {
    assert_eq!(
        Title::from_first_line("\n  Fix the login bug\nand then the rest").unwrap().as_str(),
        "Fix the login bug"
    );
    assert_eq!(Title::from_first_line("Fix it\rrm -rf").unwrap().as_str(), "Fix it");
    let long = Title::from_first_line(&"word ".repeat(30)).unwrap();
    assert!(long.ends_with("word…"), "{long}");
}

#[test]
fn a_typed_title_is_counted_in_characters() {
    assert!(Title::typed(&"é".repeat(Title::MOST_TYPED_CHARS)).is_ok());
    assert_eq!(Title::typed(&"x".repeat(Title::MOST_TYPED_CHARS + 1)), Err(TitleError::TooLong { most: 120 }));
    assert_eq!(Title::typed(" \u{200B}\u{202E} "), Err(TitleError::Empty), "invisible isn't a title");
}
