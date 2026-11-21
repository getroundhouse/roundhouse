//! What's said of a phrase.

use crate::phrases::Kind;
use crate::speakable::speakable;

fn said(phrase: &str) -> String {
    speakable(phrase, Kind::Prose)
}

/// Markdown's marks are taken out, and the words around them kept.
#[test]
fn markdowns_marks_are_taken_out() {
    assert_eq!(said("## The **plan**, in `parser.rs`\n"), "The plan, in parser.rs");
    assert_eq!(
        said("- [x] Read [the spec](docs/spec/chats.md) and ![a screen](a.png)\n"),
        "Read the spec and a screen"
    );
    assert_eq!(said("> 3. _Run_ it <br/> ~~not~~ again"), "Run it not again");
    assert_eq!(said("**Done. "), "Done.", "emphasis that closes in a phrase still to come");
    assert_eq!(said("An \\*escaped\\* star"), "An *escaped* star");
}

/// What only looks like a mark is said as it's written: code in backticks, a star or a tilde with no word beside it, an
/// underscore inside a word, and a hash with no space after it.
#[test]
fn code_and_what_only_looks_like_a_mark_are_said_as_written() {
    assert_eq!(said("Use `Vec<String>` and `__init__` here"), "Use Vec<String> and __init__ here");
    assert_eq!(said("Keep snake_case, 5 * 3 and ~/repo"), "Keep snake_case, 5 * 3 and ~/repo");
    assert_eq!(said("#2152 is open\n"), "#2152 is open");
}

/// A table's row is read as its cells, and its rule is nothing.
#[test]
fn a_table_is_read_by_its_cells() {
    assert_eq!(said("| Target | Builds |\n"), "Target, Builds");
    assert_eq!(said("|---|:---:|\n"), "");
}

/// Nothing is said of a phrase with no letter or digit in it, which the voice would make a noise of, or of a fence.
#[test]
fn nothing_is_said_of_marks_alone_or_a_fence() {
    assert_eq!(said("---\n"), "");
    assert_eq!(said("\n\n"), "");
    assert_eq!(speakable("```rust\n", Kind::Fence), "");
    assert_eq!(speakable("let a = b * 2;\n", Kind::Code), "let a = b * 2;", "code is read as it's written");
}
