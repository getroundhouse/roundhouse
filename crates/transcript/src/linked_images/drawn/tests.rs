use std::sync::Arc;

use gpui_kit::base::text::editor_host::{BlockNode, InlineNode, parse_markdown};
use gpui_kit::component::text::{MarkdownExtensions, MarkdownNode, MarkdownParseContext};
use gpui_kit::px;
use markdown::ParseOptions;
use markdown::mdast::Node;
use parking_lot::Mutex;
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;
use proptest::test_runner::{Config, RngSeed};

use super::{ALONE, AMONG_WORDS, Holds, Pointed, in_the_middle, to_read};
use crate::images_apart::images_apart;

/// What a paragraph can hold: images bare, in marks and in links, words, spaces and line breaks, tags of HTML, code,
/// math, an empty link and a reference to an image no text defines.
const IN_A_PARAGRAPH: &[&str] = &[
    "![a](a.png)",
    "**![b](b.png)**",
    "_![c](c.png)_",
    "~~![d](d.png)~~",
    "[![e](e.png)](https://example.com)",
    "[f ![g](g.png)](https://example.com)",
    "words",
    " ",
    "\n",
    "  \n",
    "<b>",
    "</b>",
    "<br>",
    "<kbd>k</kbd>",
    "`code`",
    "$x$",
    "[]()",
    "![h][h]",
];

/// What [`Holds::of`] says of a paragraph, by what its images say.
#[derive(Clone, Debug)]
enum Held {
    OnlyAnImage(String),
    Words(Vec<String>),
    TheKits,
}

/// What `Holds::of` says of each paragraph of `text`, as the kit's parser offers them, with what each was written as.
fn held(text: &str) -> Vec<(String, Held)> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let noting = Arc::clone(&seen);
    let note = move |node: &Node, cx: &MarkdownParseContext<'_>| {
        if let Node::Paragraph(paragraph) = node {
            let held = match Holds::of(&paragraph.children) {
                Holds::OnlyAnImage(image) => Held::OnlyAnImage(image.alt.clone()),
                Holds::Words(images) => Held::Words(images.into_iter().map(|image| image.alt.clone()).collect()),
                Holds::TheKits => Held::TheKits,
            };
            noting.lock().push((cx.node_source(node).unwrap_or_default().to_string(), held));
        }
        None
    };
    parse_markdown(text, Arc::new(MarkdownExtensions::default().block_parser(note))).unwrap();
    seen.lock().clone()
}

/// What the kit's parser makes of `paragraph` on its own, if it's one paragraph: what's in it that draws something.
fn the_kits(paragraph: &str) -> Option<Vec<InlineNode>> {
    let blocks = parse_markdown(paragraph, Arc::default()).ok()?;
    let [BlockNode::Paragraph(paragraph)] = blocks.as_slice() else { return None };
    let draws = |node: &&InlineNode| node.as_image().is_some() || !node.text().is_empty();
    Some(paragraph.children().iter().filter(draws).cloned().collect())
}

/// What the images in `nodes` that aren't in a link say, in order.
fn bare(nodes: &[InlineNode]) -> Vec<String> {
    let images = nodes.iter().filter_map(InlineNode::as_image).filter(|image| image.link.is_none());
    images.map(|image| image.alt.as_ref().map_or_default(ToString::to_string)).collect()
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        cases: 2000,
        ..Config::default()
    })]

    /// `Holds` says of a paragraph what the kit draws of it: one image whole when it says it's only that, and its
    /// images among words, at the height of the text, when it says it holds words, each that isn't in a link taken.
    #[test]
    fn what_a_paragraph_holds_is_what_the_kit_draws_of_it(pieces in vec(select(IN_A_PARAGRAPH), 1..12)) {
        let text = images_apart(&pieces.concat()).into_owned();
        for (paragraph, held) in held(&text) {
            let Some(kits) = the_kits(&paragraph) else { continue };
            match held {
                Held::OnlyAnImage(alt) => {
                    prop_assert_eq!(kits.len(), 1, "only an image, drawn whole: {:?}", paragraph);
                    prop_assert_eq!(bare(&kits), [alt], "and not in a link: {:?}", paragraph);
                }
                Held::Words(images) if !images.is_empty() => {
                    let words = kits.iter().any(|node| node.as_image().is_none());
                    prop_assert!(words, "drawn at the height of the text: {:?}", paragraph);
                    prop_assert_eq!(bare(&kits), images, "each image not in a link: {:?}", paragraph);
                }
                Held::Words(_) | Held::TheKits => {}
            }
        }
    }
}

/// A text that goes on is parsed again from near its end, so an image is known by where it starts in all of the text,
/// and no two share a name.
#[test]
fn an_image_the_transcript_draws_is_taken_as_its_text_is_parsed_and_known_by_where_it_starts_in_all_of_it() {
    let text = "![alone](a.png)\n\nWords ![among](b.png) and [![a link](c.png)](https://example.com)\n\n\
                # ![heading](d.png)";
    let from = 100;

    let blocks = parse_markdown(text, to_read(from)).unwrap();

    let taken = |node: &MarkdownNode| (node.name().to_string(), node.data::<Pointed>().map(|image| image.at));
    let alone: Vec<_> = (blocks.iter())
        .filter_map(|block| if let BlockNode::Custom { node, .. } = block { Some(taken(node)) } else { None })
        .collect();
    let line = |block: &'_ BlockNode| {
        if let BlockNode::Paragraph(line) | BlockNode::Heading { children: line, .. } = block {
            Some(line.children().to_vec())
        } else {
            None
        }
    };
    let in_lines: Vec<InlineNode> = blocks.iter().filter_map(line).flatten().collect();
    let among_words: Vec<_> = in_lines.iter().filter_map(|node| node.as_custom()).map(taken).collect();
    assert_eq!(alone, [(ALONE.to_string(), Some(from))]);
    assert_eq!(among_words, [(AMONG_WORDS.to_string(), text.find("![among").map(|at| from + at))]);
    assert_eq!(in_lines.iter().filter(|node| node.as_image().is_some()).count(), 2, "a link's and a heading's");
}

/// What `text`'s first paragraph holds, by what its images say.
fn holds(text: &str) -> String {
    let root = markdown::to_mdast(text, &ParseOptions::gfm()).unwrap();
    let Some(Node::Paragraph(paragraph)) = root.children().and_then(|blocks| blocks.first()) else {
        panic!("{text} doesn't start with a paragraph");
    };
    match Holds::of(&paragraph.children) {
        Holds::OnlyAnImage(image) => format!("only {}", image.alt),
        Holds::Words(images) if images.is_empty() => "words".to_string(),
        Holds::Words(images) => {
            let say: Vec<&str> = images.into_iter().map(|image| image.alt.as_str()).collect();
            format!("words and {}", say.join(", "))
        }
        Holds::TheKits => "the kit's".to_string(),
    }
}

#[test]
fn an_image_is_alone_in_a_paragraph_however_its_marked() {
    assert_eq!(holds("![board](a.png)"), "only board");
    assert_eq!(holds("**![board](a.png)**"), "only board", "in bold");
    assert_eq!(holds("_~~![board](a.png)~~_"), "only board", "struck, in italics");
}

#[test]
fn an_image_is_among_a_paragraphs_words_however_its_marked_unless_its_in_a_link() {
    assert_eq!(
        holds("It's ![the board](a.png) now, and **![bold](b.png)** _too ![deep](c.png)_"),
        "words and the board, bold, deep"
    );
    assert_eq!(holds("**![before](a.png)** **![after](b.png)**"), "words and before, after", "a space is words");
    assert_eq!(holds("See [the ![board](a.png) here](board.md)"), "words", "among a link's words");
    assert_eq!(holds("See [![board](a.png)][board]\n\n[board]: board.md"), "words", "in a link by name");
    assert_eq!(holds("See ![the board][board]\n\n[board]: a.png"), "words", "one that refers to its address");
    assert_eq!(holds("![ours](a.png) and [![badge](b.png)](https://example.com)"), "words and ours");
    assert_eq!(holds("Press <kbd>![return](a.png)</kbd>"), "words and return", "words beside a tag are words");
}

#[test]
fn a_paragraph_of_images_that_isnt_one_alone_is_the_kits() {
    assert_eq!(holds("[![Build](badge.png)](https://example.com)"), "the kit's", "all a link shows");
    assert_eq!(holds("![the board][board]\n\n[board]: a.png"), "the kit's", "one that refers to its address");
    assert_eq!(holds("**![before](a.png)![after](b.png)**"), "the kit's", "two with nothing between, in one mark");
    assert_eq!(holds("**![ours](a.png)**[![badge](b.png)](https://example.com)"), "the kit's", "one of them a link");
    assert_eq!(holds("<b>![board](a.png)</b>"), "the kit's", "a tag, which only the kit can say the words of");
}

#[test]
fn something_set_in_the_middle_of_a_line_is_as_far_above_the_baseline_whichever_sign_the_descent_has() {
    let (height, ascent, descent) = (px(18.), px(14.), px(4.));

    assert_eq!(in_the_middle(height, ascent, descent), px(14.));
    assert_eq!(in_the_middle(height, ascent, -descent), px(14.), "as the text system headless tests draw with has it");
    assert_eq!(in_the_middle(px(4.), ascent, descent), px(4.), "never outside what it's the baseline of");
}
