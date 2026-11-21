//! Which phrase is made next, how far each text has been heard as marks come back, and what a stop leaves.

use crate::reader::{AHEAD, Phrase, Reader};
use crate::{TextId, Update};

const REPLY: TextId = TextId(1);
const NEXT: TextId = TextId(2);

/// Every phrase ready to make, until none is.
fn ready(reader: &mut Reader) -> Vec<Phrase> {
    std::iter::from_fn(|| reader.next()).collect()
}

/// What `updates` say, as text, to compare.
fn said(updates: &[Update]) -> Vec<String> {
    updates.iter().map(|update| format!("{update:?}")).collect()
}

/// A text's places come as each phrase's mark comes back, not as it's made, and its end once its last is heard.
#[test]
fn a_text_is_heard_as_its_marks_come_back_and_finishes_with_its_last() {
    let mut reader = Reader::default();
    reader.say(REPLY, 0, "Sure, it's **done**. Ship");
    reader.end(REPLY);

    let phrases = ready(&mut reader);
    let says: Vec<&str> = phrases.iter().map(|phrase| phrase.say.as_str()).collect();
    assert_eq!(says, ["Sure,", "it's done.", "Ship"]);

    assert_eq!(said(&reader.reached(phrases[0].mark)), ["Heard { text: TextId(1), through: 6 }"]);
    assert_eq!(
        said(&reader.reached(phrases[2].mark)),
        [
            "Heard { text: TextId(1), through: 21 }",
            "Heard { text: TextId(1), through: 25 }",
            "Finished { text: TextId(1) }"
        ],
        "a later mark means the ones before it were played too"
    );
    assert_eq!(said(&reader.reached(phrases[1].mark)), [] as [String; 0], "once");
}

/// No more than [`AHEAD`] phrases are made before the first of them is heard.
#[test]
fn only_a_few_phrases_are_made_ahead_of_what_is_heard() {
    let mut reader = Reader::default();
    reader.say(REPLY, 0, &"One. ".repeat(AHEAD + 2));

    let made = ready(&mut reader);
    assert_eq!(made.len(), AHEAD);
    reader.reached(made[0].mark);
    assert_eq!(ready(&mut reader).len(), 1);
}

/// A text is read only once every text given before it has been cut whole, so two replies never mix.
#[test]
fn texts_are_read_one_after_another() {
    let mut reader = Reader::default();
    reader.say(REPLY, 0, "First, then");
    reader.say(NEXT, 0, "Second.");
    reader.end(NEXT);

    assert_eq!(ready(&mut reader).len(), 1, "the reply's rest may still come");
    reader.end(REPLY);
    let says: Vec<String> = ready(&mut reader).into_iter().map(|phrase| phrase.say).collect();
    assert_eq!(says, ["then", "Second."]);
}

/// A stop says which text was being read and how far it was heard, and which never started, and a mark from before
/// it matches nothing after.
#[test]
fn a_stop_says_where_the_text_was_cut_and_which_never_started() {
    let mut reader = Reader::default();
    reader.say(REPLY, 0, "One, two. Three.");
    reader.end(REPLY);
    reader.say(NEXT, 0, "Later.");
    let made = ready(&mut reader);
    reader.reached(made[0].mark);

    assert_eq!(format!("{:?}", reader.stop()), "Stopped { cut: Some((TextId(1), 5)), never_started: [TextId(2)] }");
    assert_eq!(said(&reader.reached(made[1].mark)), [] as [String; 0]);

    reader.say(NEXT, 0, "Fresh.");
    reader.end(NEXT);
    let after = ready(&mut reader);
    assert!(after.iter().all(|phrase| made.iter().all(|before| before.mark != phrase.mark)), "marks are new");
    assert_eq!(
        format!("{:?}", reader.stop()),
        "Stopped { cut: Some((TextId(2), 0)), never_started: [] }",
        "begun, though none of it was heard yet"
    );
    assert_eq!(format!("{:?}", reader.stop()), "Stopped { cut: None, never_started: [] }");

    reader.say(REPLY, 0, "Still coming");
    assert!(reader.next().is_none());
    assert_eq!(
        format!("{:?}", reader.stop()),
        "Stopped { cut: None, never_started: [TextId(1)] }",
        "given, but none of it made yet"
    );
}

/// A phrase with nothing to say still has its place, so a table's rule is heard in its turn, and an empty text ends.
#[test]
fn a_phrase_with_nothing_to_say_still_has_its_place() {
    let mut reader = Reader::default();
    reader.say(REPLY, 0, "|---|\nNext.");
    reader.end(REPLY);
    reader.say(NEXT, 0, "");
    reader.end(NEXT);

    let made = ready(&mut reader);
    let says: Vec<&str> = made.iter().map(|phrase| phrase.say.as_str()).collect();
    assert_eq!(says, ["", "Next.", ""]);
    assert_eq!(
        said(&reader.reached(made[2].mark)),
        [
            "Heard { text: TextId(1), through: 6 }",
            "Heard { text: TextId(1), through: 11 }",
            "Finished { text: TextId(1) }",
            "Finished { text: TextId(2) }",
        ],
    );
}

/// A text that ends only after its last phrase was made still finishes, once that phrase is heard, or after it if it
/// already was.
#[test]
fn a_text_ended_after_its_last_phrase_was_made_still_finishes() {
    let mut reader = Reader::default();
    reader.say(REPLY, 0, "Done.\n");
    let made = ready(&mut reader);
    reader.end(REPLY);
    let closing = ready(&mut reader);
    assert_eq!(said(&reader.reached(made[0].mark)), ["Heard { text: TextId(1), through: 6 }"]);
    assert_eq!(said(&reader.reached(closing[0].mark)), ["Finished { text: TextId(1) }"]);

    reader.say(NEXT, 0, "Done.\n");
    let made = ready(&mut reader);
    assert_eq!(said(&reader.reached(made[0].mark)), ["Heard { text: TextId(2), through: 6 }"]);
    reader.end(NEXT);
    let closing = ready(&mut reader);
    assert_eq!(closing.len(), 1);
    assert_eq!(closing[0].say, "");
    assert_eq!(said(&reader.reached(closing[0].mark)), ["Finished { text: TextId(2) }"]);
    assert!(!reader.is_reading());
}

/// What's given of a text a stop forgot, or that doesn't follow on from what was given, is dropped, so a place is
/// never counted from the middle of a text.
#[test]
fn what_doesnt_follow_on_from_a_text_is_dropped() {
    let mut reader = Reader::default();
    reader.say(REPLY, 0, "One. ");
    ready(&mut reader);
    reader.stop();
    reader.say(REPLY, 5, "Two. ");
    reader.end(REPLY);
    assert!(reader.next().is_none() && !reader.is_reading(), "the rest of a text it stopped");

    reader.say(NEXT, 0, "One. ");
    reader.say(NEXT, 9, "Three.");
    reader.say(NEXT, 2, "e. ");
    reader.say(NEXT, 5, "Two.");
    reader.end(NEXT);
    let says: Vec<String> = ready(&mut reader).into_iter().map(|phrase| phrase.say).collect();
    assert_eq!(says, ["One.", "Two."], "a gap and an overlap are dropped");
}
