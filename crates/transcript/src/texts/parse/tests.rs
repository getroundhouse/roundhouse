use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;
use proptest::test_runner::{Config, RngSeed};

use gpui_kit::base::text::editor_host::BlockNode;

use super::ImagesAre::{self, TheKits, TheTranscripts};
use super::{ATOM, Parsed, again, gone_on, leaves, parse};
use crate::linked_images;

/// Bits of Markdown whose blocks a text that goes on can change: a paragraph a line of `=` makes a heading, lists,
/// fences that are open, some of them indented, a definition before its use, a footnote and words beyond ASCII. With
/// [`TABLES`] or [`QUOTES`], but not both, since the kit's parser reads a table after a quote's lazy line differently
/// in a whole text, as [`super::again`] says.
const BITS: &[&str] = &[
    "Some **bold** and `code` text",
    "Café, naïve and ✓ ",
    " that goes on\n",
    "\n\n",
    "\n",
    "===\n",
    "# A heading\n",
    "- one\n- two\n",
    "  - nested\n",
    "1. first\n",
    "lazy line\n",
    "```rust\nlet x = 1;\n",
    "```\n",
    " ```\n",
    "  ```sh\n",
    "[docs]: https://example.com\n",
    "See [the docs][docs].",
    "![a picture](a.png)",
    "A note[^1].",
    "[^1]: The note.\n",
    "<div>\n",
    "</div>\n",
    "---\n",
    "    indented code\n",
    "  continued\n",
    "- ",
    "1",
    ". ",
    "* star\n",
];

const TABLES: &[&str] = &["| a | b |\n", "|---|---|\n", "| c | d |\n"];

const QUOTES: &[&str] = &["> quoted\n", ">\n", "> > nested\n"];

/// Images the transcript draws when it can, and some it leaves to the kit, beside what a paragraph can hold.
const IMAGES: &[&str] =
    &["**![b](b.png)**", "[![e](e.png)](https://example.com)", "\n\n![alone](a.png)\n\n", "_![c](c.png)_ ", "<br>"];

/// `bits` and `more`, each as likely as the other.
fn with(more: &'static [&'static str]) -> Vec<&'static str> {
    BITS.iter().chain(more).copied().collect()
}

/// What `parsed` would draw, but for where the kit's parser says each block and its words are in what it parsed,
/// which, for a text parsed again from its last block, is from there.
fn drawn(parsed: &Parsed) -> String {
    let mut out = format!("{:?} {:?} {:?}", parsed.blocks, parsed.leaves, parsed.links);
    for (place, ends) in [("span: Some(", ")"), ("source: ", " ")] {
        let mut from = 0;
        while let Some(at) = out[from..].find(place).map(|at| from + at + place.len()) {
            let end = out[at..].find(ends).map_or(out.len(), |end| at + end);
            let end = if ends == " " { end.saturating_sub(1).max(at) } else { end };
            out.replace_range(at..end, "_");
            from = at;
        }
    }
    out
}

/// What `was` parsed and then gone on to `source` is drawn as, and what `source` parsed whole is.
fn going_on(was: &str, source: &str) -> (String, String) {
    let was = parse(was.to_owned(), TheKits);
    let gone = match again(&was, source) {
        Some(again) => gone_on(&was, again, source.to_owned()),
        None => parse(source.to_owned(), TheKits),
    };
    (drawn(&gone), drawn(&parse(source.to_owned(), TheKits)))
}

#[test]
fn a_list_takes_in_an_item_after_a_blank_line_as_it_goes_on() {
    let (gone, whole) = going_on("1. first\n\n\n1", "1. first\n\n\n1. second\n");

    assert_eq!(gone, whole);
}

#[test]
fn a_paragraph_takes_back_what_turns_out_not_to_be_a_heading_as_it_goes_on() {
    let (gone, whole) = going_on("Some text\n#", "Some text\n#2 isn't a heading");

    assert_eq!(gone, whole);
}

#[test]
fn a_text_that_goes_on_to_define_a_link_is_parsed_whole() {
    let was = "See [docs].\n\nAnd more.\n\n";

    let (gone, whole) = going_on(was, &format!("{was}[docs]: https://example.com\n"));

    assert_eq!(gone, whole);
}

#[test]
fn an_indented_fence_is_parsed_again_from_the_start_of_its_line() {
    let (gone, whole) = going_on("Run this:\n\n  ```sh\n  cargo test", "Run this:\n\n  ```sh\n  cargo test\n  ```\n");

    assert_eq!(gone, whole);
}

#[test]
fn a_text_that_goes_on_to_define_a_link_over_two_lines_is_parsed_whole() {
    let was = "See [foo bar].\n\nAnd more.\n\n[foo\n";

    let (gone, whole) = going_on(was, &format!("{was}bar]: https://example.com\n"));

    assert_eq!(gone, whole);
}

#[test]
fn a_text_that_defines_a_link_is_parsed_whole_as_it_goes_on() {
    let was = "[docs]: https://example.com\n\n- one\n- two";

    let (gone, whole) = going_on(was, &format!("{was}\n- [docs]"));

    assert_eq!(gone, whole);
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    /// However a text with tables comes in, parsing it again from near its end each time it goes on draws what
    /// parsing it whole would.
    #[test]
    fn a_text_with_tables_parsed_as_it_goes_on_is_drawn_as_it_would_be_parsed_whole(
        bits in vec(select(with(TABLES)), 1..24),
        cuts in vec(0..usize::MAX, 1..8),
    ) {
        let (gone, whole, text) = in_pieces(&bits, &cuts);
        prop_assert_eq!(gone, whole, "{:?}", text);
    }

    /// The same for a text with quotes.
    #[test]
    fn a_text_with_quotes_parsed_as_it_goes_on_is_drawn_as_it_would_be_parsed_whole(
        bits in vec(select(with(QUOTES)), 1..24),
        cuts in vec(0..usize::MAX, 1..8),
    ) {
        let (gone, whole, text) = in_pieces(&bits, &cuts);
        prop_assert_eq!(gone, whole, "{:?}", text);
    }

    /// Find counts what a text shows from its leaves as parsed for the kit to draw its images, and the selection
    /// keeps them, whoever draws them.
    #[test]
    fn a_text_has_the_same_leaves_whoever_draws_its_images(bits in vec(select(with(IMAGES)), 1..24)) {
        let text = bits.concat();
        prop_assert_eq!(parse(text.clone(), TheTranscripts).leaves, parse(text.clone(), TheKits).leaves, "{:?}", text);
    }
}

/// What `bits`, together, are drawn as when they come in cut where `cuts` say, parsed again from near the end each time
/// they go on, and when they're parsed whole, and what they say.
fn in_pieces(bits: &[&str], cuts: &[usize]) -> (String, String, String) {
    let text = bits.concat();
    let cuts = cuts.iter().map(|cut| cut % text.len()).filter(|&cut| text.is_char_boundary(cut));
    let mut cuts: Vec<usize> = cuts.collect();
    cuts.sort_unstable();
    cuts.push(text.len());
    let mut parsed = parse(String::new(), TheKits);
    for cut in cuts {
        let source = text[..cut].to_owned();
        parsed = match again(&parsed, &source) {
            Some(again) => gone_on(&parsed, again, source),
            None => parse(source, TheKits),
        };
    }
    (drawn(&parsed), drawn(&parse(text.clone(), TheKits)), text)
}

#[test]
fn each_paragraph_heading_code_block_and_cell_is_a_leaf_in_order() {
    let source = "# Title\n\nSome **bold** text.\n\n- one\n- two\n\n```rust\nlet x = 1;\n```\n\n| a | b |\n|---|---|\n| c | d |\n";

    assert_eq!(*leaves(source), ["Title", "Some bold text.", "one", "two", "let x = 1;", "a", "b", "c", "d"]);
}

#[test]
fn an_image_in_text_is_one_character_of_its_leaf_and_one_alone_is_no_leaf() {
    let source = "Before ![a picture](a.png) after\n\n![alone](b.png)\n\nThe end";

    assert_eq!(*leaves(source), [format!("Before {ATOM} after"), "The end".to_string()]);
}

#[test]
fn a_texts_images_are_the_transcripts_to_draw_only_if_it_has_any_and_they_can_be_looked_up() {
    assert_eq!(ImagesAre::of("The board, after: ![The board](board.png)", true), TheTranscripts);
    assert_eq!(ImagesAre::of("The board, after: ![The board][board]", true), TheTranscripts, "by reference too");
    assert_eq!(ImagesAre::of("The board is red at stop now.", true), TheKits, "none to draw");
    assert_eq!(ImagesAre::of("![The board](board.png)", false), TheKits, "none to look up");
}

/// A text that goes on is parsed again from near its end, and the images taken from what's parsed again are known by
/// where they start in all of it, as a whole parse knows them, so no two share a name.
#[test]
fn a_text_parsed_again_from_near_its_end_knows_its_images_by_where_they_start_in_all_of_it() {
    let was = "The board, before the fix.\n\nAnd";
    let source = format!("{was} after it, ![among words](a.png) and on a line of its own:\n\n![alone](b.png)");
    let was = parse(was.to_owned(), TheTranscripts);
    let again = again(&was, &source).expect("parsed again from its last block");

    let gone = gone_on(&was, again, source.clone());

    assert!(again.byte > 0, "from part of the way in");
    assert_eq!(taken(&gone), [source.find("![among"), source.find("![alone")]);
    assert_eq!(taken(&gone), taken(&parse(source, TheTranscripts)), "as a whole parse has them");
}

/// Where each image the transcript took from `parsed` starts in its text, in order.
fn taken(parsed: &Parsed) -> Vec<Option<usize>> {
    let mut taken = Vec::new();
    for block in &parsed.blocks {
        if let BlockNode::Custom { node, .. } = block {
            taken.push(linked_images::taken_at(node));
        } else if let BlockNode::Paragraph(line) = block {
            let among_words = line.children().iter().filter_map(|node| node.as_custom());
            taken.extend(among_words.map(linked_images::taken_at));
        }
    }
    taken
}

#[test]
fn a_reference_link_goes_where_its_first_definition_says_even_inside_a_quote() {
    let source = "See [the docs][docs].\n\n> [docs]: https://example.com/first\n\n[docs]: https://example.com/second";

    let links = parse(source.to_owned(), TheKits).links;

    assert_eq!(links.get("docs").map(|link| link.url.as_ref()), Some("https://example.com/first"));
    assert_eq!(*leaves(source), ["See the docs."], "and a definition is no leaf");
}

#[test]
fn a_text_the_parser_panics_on_is_one_paragraph_as_it_is_written() {
    let source = "a\n===\n---\nb\n===";

    assert_eq!(*leaves(source), [source]);
}

#[test]
fn a_link_is_its_words_and_not_its_address() {
    assert_eq!(*leaves("See [the docs](https://example.com) and `cargo test`."), ["See the docs and cargo test."]);
}
