//! How far a character and a word go, and which end a key moves, and where to.

use gpui_kit::TestAppContext;
use protocol::model::Seq;

use super::super::{Step, Way, character_beside, word_beside};
use super::support::{place, selecting, step};
use crate::texts::{TextAt, TextKey, Texts};

#[test]
fn a_character_is_what_a_person_sees_as_one() {
    // An e and its accent, which are two characters to Rust, and a flag, which is two of four bytes each.
    let text = "ae\u{301}b🇬🇧";

    assert_eq!(character_beside(text, 1, Way::On), Some(4), "past the e and its accent together");
    assert_eq!(character_beside(text, 4, Way::Back), Some(1), "and back");
    assert_eq!(character_beside(text, 5, Way::On), Some(text.len()), "past the flag whole");
    assert_eq!(character_beside(text, text.len(), Way::On), None, "and nothing past the end");
    assert_eq!(character_beside(text, 0, Way::Back), None, "or before the start");
}

#[test]
fn a_word_at_a_time_goes_to_the_end_of_the_word_after_or_the_start_of_the_one_before() {
    let text = "the quick_fox, jumps";

    assert_eq!(word_beside(text, 0, Way::On), Some(3), "to the end of the word it's at the start of");
    assert_eq!(word_beside(text, 3, Way::On), Some(13), "past the space to the end of the next");
    assert_eq!(word_beside(text, 6, Way::On), Some(13), "and from inside one, to its end");
    assert_eq!(word_beside(text, 13, Way::On), Some(text.len()), "past what isn't in a word");
    assert_eq!(word_beside(text, text.len(), Way::On), None, "and nothing at the end");
    assert_eq!(word_beside(text, text.len(), Way::Back), Some(15), "back to the start of the last");
    assert_eq!(word_beside(text, 15, Way::Back), Some(4), "and past what isn't in a word to the one before");
    assert_eq!(word_beside("ends.", 5, Way::Back), Some(0), "a word before what isn't in one");
    assert_eq!(word_beside("  ", 0, Way::On), Some(2), "and the edge, with no word that way");
}

/// The end moved last is the one a key moves, and the other stays: here from a selection made backwards, which a key
/// makes longer at its start.
#[gpui_kit::test]
fn a_key_moves_the_end_that_moved_last(cx: &mut TestAppContext) {
    let texts = Texts::alone();
    let (selection, window) = selecting(cx, &texts);
    let reply = TextKey::named("reply");
    texts.drawn_as(&reply, TextAt::only(Seq::new(1), 0), "Signals clear ahead");
    selection.inner.borrow_mut().set(Some((place(&reply, 0, 13), place(&reply, 0, 8))));

    step(&selection, window, Step::Word(Way::Back));
    let made_longer = selection.text();
    step(&selection, window, Step::Character(Way::On));

    assert_eq!(made_longer, "Signals clear");
    assert_eq!(selection.text(), "ignals clear", "and a character back from there");
}

/// Past the end of a paragraph, or any other leaf, a character is the start of the next, and a word the end of its
/// first word, whether the text's drawn or not.
#[gpui_kit::test]
fn past_the_end_of_a_leaf_is_the_start_of_the_next(cx: &mut TestAppContext) {
    let texts = Texts::alone();
    let (selection, window) = selecting(cx, &texts);
    let reply = TextKey::named("reply");
    texts.drawn_as(&reply, TextAt::only(Seq::new(1), 0), "Signals clear\n\nTrains wait");
    selection.inner.borrow_mut().set(Some((place(&reply, 0, 8), place(&reply, 0, 13))));
    let far = || selection.inner.borrow().ends.clone().map(|(_, far)| far);

    step(&selection, window, Step::Character(Way::On));
    let next = far();
    step(&selection, window, Step::Word(Way::On));

    assert_eq!(next, Some(place(&reply, 1, 0)));
    assert_eq!(far(), Some(place(&reply, 1, 6)), "and the end of its first word");
}

/// Everything ends past the end of the last text, which a key takes as the end of what it shows.
#[gpui_kit::test]
fn a_key_after_selecting_everything_moves_from_the_end_of_what_the_last_text_shows(cx: &mut TestAppContext) {
    let texts = Texts::alone();
    let (selection, window) = selecting(cx, &texts);
    texts.drawn_at(TextAt::only(Seq::new(1), 0), "Signals clear");
    texts.drawn_at(TextAt::only(Seq::new(2), 0), "Trains wait");
    let everything = selection.whole();
    selection.inner.borrow_mut().set(everything);

    step(&selection, window, Step::Character(Way::Back));

    assert_eq!(selection.text(), "Signals clear\n\nTrains wai");
}
