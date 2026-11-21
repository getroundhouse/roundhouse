//! Where a text is cut into phrases as it comes.

use crate::phrases::{Cutting, Kind, LONGEST, Piece};

/// Every phrase `text` is cut into once it has all come, as the text it covers.
fn cut_whole(text: &str) -> Vec<String> {
    let mut cutting = Cutting::default();
    cutting.push(text);
    cutting.end();
    std::iter::from_fn(|| cutting.next()).map(|piece| text[piece.range].to_string()).collect()
}

/// The first phrase ends at its first comma, so the voice starts at once, and each after it at a sentence's end, with
/// the spaces after it.
#[test]
fn the_first_phrase_ends_at_a_comma_and_the_rest_at_a_sentences_end() {
    assert_eq!(
        cut_whole("Sure, I'll look. It's in the hub, I think! Is that right? Yes"),
        ["Sure, ", "I'll look. ", "It's in the hub, I think! ", "Is that right? ", "Yes"],
    );
}

/// A full stop that isn't followed by a space doesn't end a sentence, and a list's number doesn't either. Closing
/// marks go with the end before them.
#[test]
fn only_a_stop_before_a_space_ends_a_phrase_and_closing_marks_go_with_it() {
    assert_eq!(
        cut_whole("It's v1.2.3 now. **Done.** Then \"go.\" Next"),
        ["It's v1.2.3 now. ", "**Done.** ", "Then \"go.\" ", "Next"],
    );
    assert_eq!(cut_whole("2. Run it again\n"), ["2. Run it again\n"]);
}

/// Each line ends a phrase, and blank lines go with the phrase after them.
#[test]
fn a_line_ends_a_phrase_and_blank_lines_join_the_next() {
    assert_eq!(cut_whole("# Plan\n\n- one\n- two"), ["# Plan\n", "\n- one\n", "- two"]);
}

/// A fenced block is read a line at a time, as code, and its fences are phrases of their own.
#[test]
fn a_fenced_block_is_cut_a_line_at_a_time_as_code() {
    let text = "Here.\n```rust\nlet a = 1. b;\n```\nAfter.";
    let mut cutting = Cutting::default();
    cutting.push(text);
    cutting.end();
    let pieces: Vec<(&str, Kind)> =
        std::iter::from_fn(|| cutting.next()).map(|Piece { range, kind }| (&text[range], kind)).collect();

    assert_eq!(
        pieces,
        [
            ("Here.\n", Kind::Prose),
            ("```rust\n", Kind::Fence),
            ("let a = 1. b;\n", Kind::Code),
            ("```\n", Kind::Fence),
            ("After.", Kind::Prose)
        ]
    );
}

/// A phrase is cut only once what follows its end has come, so a stop that might be part of `3.5` waits, and the rest
/// waits for the text's end.
#[test]
fn a_phrase_waits_until_what_follows_it_has_come() {
    let mut cutting = Cutting::default();
    cutting.push("Version 3.");
    assert_eq!(cutting.next(), None, "it may go on as 3.5");
    cutting.push("5 is out. And");
    assert_eq!(cutting.next().map(|piece| piece.range), Some(0..20), "Version 3.5 is out. ");
    assert_eq!(cutting.next(), None, "the rest may go on");
    assert!(!cutting.finished());
    cutting.end();
    assert_eq!(cutting.next().map(|piece| piece.range), Some(20..23), "the rest, to the text's end");
    assert!(cutting.finished());
    assert_eq!(cutting.next(), None);
}

/// A phrase with no end in sight is cut at the last space before it's too long, so a place heard moves on.
#[test]
fn a_phrase_too_long_is_cut_at_a_space() {
    let text = "word ".repeat(LONGEST / 5 * 3);
    let mut cutting = Cutting::default();
    cutting.push(&text);

    let first = cutting.next().unwrap().range;
    assert!(first.len() <= LONGEST && text[..first.end].ends_with(' '), "{first:?}");
}

/// A text that ended empty, or ended once its last phrase had already been cut, has one more phrase, with nothing in
/// it, so its end is still heard in its turn.
#[test]
fn a_text_that_ends_after_its_last_phrase_was_cut_has_an_empty_one_to_end_it() {
    let mut cutting = Cutting::default();
    assert_eq!(cutting.next(), None, "it may still come");
    cutting.end();
    assert_eq!(cutting.next(), Some(Piece { range: 0..0, kind: Kind::Prose }));
    assert!(cutting.finished());
    assert_eq!(cutting.next(), None);

    let mut cutting = Cutting::default();
    cutting.push("Done.\n");
    assert_eq!(cutting.next().map(|piece| piece.range), Some(0..6));
    assert!(!cutting.finished(), "more may come");
    cutting.end();
    assert_eq!(cutting.next(), Some(Piece { range: 6..6, kind: Kind::Prose }));
    assert!(cutting.finished());
    assert_eq!(cutting.next(), None);
}
