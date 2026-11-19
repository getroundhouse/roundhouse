use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;
use proptest::test_runner::{Config, RngSeed};

use gpui_kit::base::text::editor_host::BlockNode;

use super::Whose::{self, TheKits, TheTranscripts};
use super::{ATOM, Leaf, Parsed, again, gone_on, leaves, parse, parse_shown, top_blocks};
use crate::linked_images;

/// What each of `leaves` shows.
fn shows(leaves: &[Leaf]) -> Vec<&str> {
    leaves.iter().map(Leaf::shows).collect()
}

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

/// Images the transcript draws when it can, and some it leaves to the kit, beside what a paragraph can hold, and the
/// diagrams it draws, open and closed.
const IMAGES: &[&str] = &[
    "**![b](b.png)**",
    "[![e](e.png)](https://example.com)",
    "\n\n![alone](a.png)\n\n",
    "_![c](c.png)_ ",
    "<br>",
    "```mermaid\n",
    "~~~Mermaid\n",
    "flowchart LR\n  A --> B\n",
    "~~~\n",
];

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

/// A text as it was parsed, and what it went on to, for each rule of where [`again`] parses it from. At the cases they
/// run, the laws below come on few of these, so each rule has a row of its own.
const GOING_ON: &[(&str, &str, &str)] = &[
    ("a list takes in an item after a blank line", "1. first\n\n\n1", "1. first\n\n\n1. second\n"),
    ("a paragraph takes back what turns out not to be a heading", "Some text\n#", "Some text\n#2 isn't a heading"),
    (
        "a text that goes on to define a link is parsed whole",
        "See [docs].\n\nAnd more.\n\n",
        "See [docs].\n\nAnd more.\n\n[docs]: https://example.com\n",
    ),
    (
        "an indented fence is parsed again from the start of its line",
        "Run this:\n\n  ```sh\n  cargo test",
        "Run this:\n\n  ```sh\n  cargo test\n  ```\n",
    ),
    (
        "a text that goes on to define a link over two lines is parsed whole",
        "See [foo bar].\n\nAnd more.\n\n[foo\n",
        "See [foo bar].\n\nAnd more.\n\n[foo\nbar]: https://example.com\n",
    ),
    (
        "a text that defines a link is parsed whole",
        "[docs]: https://example.com\n\n- one\n- two",
        "[docs]: https://example.com\n\n- one\n- two\n- [docs]",
    ),
];

#[test]
fn a_text_parsed_again_from_near_its_end_as_it_goes_on_is_drawn_as_it_would_be_parsed_whole() {
    for &(name, was, source) in GOING_ON {
        let (gone, whole) = going_on(was, source);

        assert_eq!(gone, whole, "{name}");
    }
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

    /// A document copies a block out of sight from what the whole text's parse says it shows, and draws it from its
    /// own parse, so each block alone shows what it shows in the whole text, and together they show all of it, for a
    /// text whose blocks are a blank line apart, as a written document's are. `top_blocks` says where they differ.
    #[test]
    fn a_text_cut_into_its_top_blocks_shows_in_them_what_it_shows_whole(blocks in vec(select(BLOCKS), 1..24)) {
        shown_in_top_blocks_as_whole(&blocks.concat())?;
    }
}

/// Whole blocks, each with a blank line after it, as a document's are: one of each kind, a definition and what refers
/// to it, footnotes and their marks, which may come in any order, one defined in a quote, which has the text drawn
/// whole, and a fence left open, which takes in what's after it.
const BLOCKS: &[&str] = &[
    "# A heading\n\n",
    "Some **bold** and `code` text,\nand Café, naïve and ✓\n\n",
    "- one\n- two\n  - nested\n\n",
    "1. first\n2. second\n\n",
    "```rust\nlet x = 1;\n```\n\n",
    "  ```sh\n  indented fence\n  ```\n\n",
    "    indented code\n\n",
    "| a | b |\n|---|---|\n| c | d |\n\n",
    "> quoted\n>\n> > nested\n\n",
    "See [the docs][docs].\n\n",
    "[docs]: https://example.com\n\n",
    "A note[^1].\n\n",
    "[^1]: The note, with **bold**.\n\n",
    "- A named one[^named], in a list\n\n",
    "[^named]: Its note.\n\n    Its second paragraph.\n\n",
    "> [^quoted]: One in a quote[^1].\n\n",
    "Its mark[^quoted].\n\n",
    "```md\n[^shown]: One in code.\n```\n\n",
    "---\n\n",
    "![a picture](a.png)\n\n",
    "<div>\nhtml\n</div>\n\n",
    "```sh\nleft open\n",
];

/// A text with footnotes is cut into its blocks like any other, so a long one draws only what's in sight, but for one
/// defined in a list or a quote, which is drawn as part of it.
#[test]
fn a_text_is_cut_into_its_blocks_with_footnotes_defined_at_its_top() {
    let defined = |at: &str| format!("A note[^1].\n\nMore.\n\n{at}[^1]: The note.\n");
    let blocks = top_blocks(&defined(""));
    let drawn: Vec<_> =
        blocks.iter().map(|block| parse_shown(block.source.clone(), TheKits, Some(block.shown))).collect();
    assert_eq!(
        drawn.iter().map(|parsed| shows(&parsed.leaves)).collect::<Vec<_>>(),
        [["A note[1]."], ["More."], ["[1]: The note."]]
    );
    assert_eq!(top_blocks(&defined("> ")).len(), 1, "one in a quote is drawn whole");
    assert_eq!(top_blocks(&defined("- ")).len(), 1, "as is one in a list");
    let shown = "How to write one:\n\n```md\nA note[^1].\n\n[^1]: The note.\n```\n\nThat's all.\n";
    assert_eq!(top_blocks(shown).len(), 3, "but one in code is only words");
}

/// A block is parsed with only the definitions it refers to, since a text such as a changelog can define hundreds.
#[test]
fn a_block_is_parsed_with_only_the_definitions_it_refers_to() {
    let text = "See [the docs][Docs].\n\n[docs]: https://example.com/docs\n\n[other]: https://example.com/other\n";
    let sources: Vec<_> = top_blocks(text).into_iter().map(|block| block.source).collect();
    assert_eq!(sources, ["See [the docs][Docs].\n\n[docs]: https://example.com/docs"]);
}

/// That each of `text`'s blocks at its top shows alone what it shows in the whole text, and together they show all of
/// it.
fn shown_in_top_blocks_as_whole(text: &str) -> Result<(), TestCaseError> {
    let blocks = top_blocks(text);
    for block in &blocks {
        let drawn = parse_shown(block.source.clone(), TheKits, Some(block.shown)).leaves;
        prop_assert_eq!(&drawn, &block.leaves, "{:?} in {:?}", block.source, text);
    }
    let all: Vec<Leaf> = blocks.iter().flat_map(|block| block.leaves.iter().cloned()).collect();
    prop_assert_eq!(all.as_slice(), &*leaves(text), "{:?}", text);
    Ok(())
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
fn a_text_s_leaves_are_its_blocks_of_words_in_order_as_they_show() {
    let cases = [
        (
            "each paragraph, heading, code block and cell",
            "# Title\n\nSome **bold** text.\n\n- one\n- two\n\n```rust\nlet x = 1;\n```\n\n| a | b |\n|---|---|\n| c | d |\n",
            ["Title", "Some bold text.", "one", "two", "let x = 1;", "a", "b", "c", "d"].map(String::from).to_vec(),
        ),
        (
            "an image in text is one character of its leaf, and one alone is no leaf",
            "Before ![a picture](a.png) after\n\n![alone](b.png)\n\nThe end",
            vec![format!("Before {ATOM} after"), "The end".to_string()],
        ),
        (
            "a link is its words and not its address",
            "See [the docs](https://example.com) and `cargo test`.",
            vec!["See the docs and cargo test.".to_string()],
        ),
    ];

    for (name, source, want) in cases {
        assert_eq!(shows(&leaves(source)), want, "{name}");
    }
}

#[test]
fn what_a_text_points_to_is_the_transcripts_to_draw_only_if_it_has_any_and_it_can_be_looked_up() {
    assert_eq!(Whose::of("The board, after: ![The board](board.png)", true), TheTranscripts);
    assert_eq!(Whose::of("The board, after: ![The board][board]", true), TheTranscripts, "by reference too");
    assert_eq!(Whose::of("Fixed in [#12](https://github.com/o/r/pull/12).", true), TheTranscripts, "or a link to one");
    assert_eq!(Whose::of("The board is red at stop now, as #12 says.", true), TheKits, "none to draw");
    assert_eq!(Whose::of("![The board](board.png)", false), TheKits, "none to look up");
}

/// In the transcript's parse, a link to an issue or a pull request is one [`ATOM`], which says the link's words, so
/// it's found and copied as them. In the kit's, which is what the person's prompts and the agent's thoughts are parsed
/// as, it's a link like any other.
#[test]
fn a_link_to_an_issue_is_one_atom_that_says_its_words_only_in_the_transcripts_parse() {
    let source = "Fixed in [the\nfix](https://github.com/o/r/pull/12), and [#3](https://github.com/o/r/issues/3).";
    let theirs = parse(source.to_owned(), TheTranscripts);
    assert_eq!(shows(&theirs.leaves), [format!("Fixed in {ATOM}, and {ATOM}.")]);
    let kits = parse(source.to_owned(), TheKits);
    assert_eq!(shows(&kits.leaves), ["Fixed in the fix, and #3."]);
    assert_eq!(theirs.leaves[0].said(), kits.leaves[0].shows(), "so find counts what it marks");
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
    assert_eq!(shows(&leaves(source)), ["See the docs."], "and a definition is no leaf");
}
