use std::borrow::Cow;

use markdown::ParseOptions;
use markdown::mdast::Node;
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;
use proptest::test_runner::{Config, RngSeed};

use super::{an_image_may_follow_another, images_apart, is_image, is_space};

/// How [`outline`] writes a paragraph that's only images.
#[derive(Clone, Copy)]
enum Paragraphs {
    /// As it is: `Paragraph[A B]`.
    AsTheyAre,
    /// As `images_apart` should make it, a paragraph for each image: `Paragraph[A] Paragraph[B]`.
    Apart,
}

/// The blocks the text view reads `text` as, each by its kind with what's in it in brackets, and each image by its alt
/// text, so `Blockquote[Paragraph[A] Paragraph[B]]` is a quote of two paragraphs that are an image each.
fn blocks(text: &str) -> String {
    blocks_with(text, Paragraphs::AsTheyAre)
}

fn blocks_with(text: &str, paragraphs: Paragraphs) -> String {
    let root = markdown::to_mdast(text, &ParseOptions::gfm()).unwrap();
    root.children().unwrap().iter().map(|block| outline(block, paragraphs)).collect::<Vec<_>>().join(" ")
}

fn outline(node: &Node, paragraphs: Paragraphs) -> String {
    if let Node::Image(image) = node {
        return image.alt.clone();
    }
    if let Node::Text(text) = node {
        return text.value.trim().to_string();
    }
    if let Node::Break(_) = node {
        return String::new();
    }
    if let (Node::Paragraph(paragraph), Paragraphs::Apart) = (node, paragraphs)
        && paragraph.children.iter().all(|child| is_image(child) || is_space(child))
    {
        let images = paragraph.children.iter().filter(|child| is_image(child));
        return images.map(|image| format!("Paragraph[{}]", outline(image, paragraphs))).collect::<Vec<_>>().join(" ");
    }
    let kind = format!("{node:?}");
    let kind = kind.split([' ', '(']).next().unwrap();
    let inside: Vec<String> = node
        .children()
        .into_iter()
        .flatten()
        .map(|child| outline(child, paragraphs))
        .filter(|it| !it.is_empty())
        .collect();
    format!("{kind}[{}]", inside.join(" "))
}

/// A few lines, each starting with what puts it in list items, quotes and footnotes, or with nothing or spaces, and
/// holding a few images, as links or references too, with spaces or a word between them. Each image's alt text is
/// its own, such as `I0`, so [`blocks`] tells them apart.
fn text() -> impl Strategy<Value = String> {
    let start = select(vec![
        "",
        "  ",
        "    ",
        "\t",
        ">",
        "> ",
        ">   ",
        "> > ",
        "- ",
        "* ",
        "  - ",
        // Not `10) `: to this parser, a list starting past 1 cuts a paragraph short only when the paragraph comes right
        // after a list, which the last image's paragraph no longer does once the images are apart.
        "1. ",
        "1) ",
        "- [ ] ",
        "> - [x] ",
        "[^1]: ",
        "[^note]: ",
        "[^note]: - ",
        "- > ",
        "-\t",
        ">\t",
    ]);
    let piece = select(vec!["![@](a.png)", "[![@](a.png)](https://example.com)", "![@][a]", "word"]);
    let between = select(vec!["", " ", "  ", "\t"]);
    let end = select(vec!["\n", "\n\n", "  \n", "\\\n", "\r\n"]);
    vec((start, vec((piece, between), 1..4), end), 1..6).prop_map(|lines| {
        let mut text = String::new();
        let mut count = 0;
        for (start, pieces, end) in lines {
            text.push_str(start);
            for (piece, between) in pieces {
                text.push_str(&piece.replace('@', &format!("I{count}")));
                text.push_str(between);
                count += 1;
            }
            text.push_str(end);
        }
        text + "\n\n[a]: a.png\n"
    })
}

proptest! {
    #![proptest_config(Config {
        rng_seed: RngSeed::Fixed(1),
        failure_persistence: None,
        ..Config::default()
    })]

    /// However images are written, each paragraph that's only images becomes a paragraph for each, in the same list
    /// items, quotes and footnotes, and nothing else changes.
    #[test]
    fn each_image_is_a_paragraph_where_it_was(text in text()) {
        let apart = images_apart(&text);

        prop_assert_eq!(blocks(&apart), blocks_with(&text, Paragraphs::Apart), "{:?} became {:?}", text, apart);
    }
}

#[test]
fn images_on_lines_of_their_own_are_each_a_paragraph() {
    let text = "Before and after:\n\n![Before](before.png)\n![After](after.png)\n\nThe badge moved.";

    assert_eq!(
        images_apart(text),
        "Before and after:\n\n![Before](before.png)\n\n![After](after.png)\n\nThe badge moved."
    );
}

#[test]
fn images_on_one_line_are_each_a_paragraph() {
    let text = "![Before](before.png) ![After](after.png)  \n[![Linked](linked.png)](https://example.com)";

    assert_eq!(
        images_apart(text),
        "![Before](before.png)\n\n![After](after.png)\n\n[![Linked](linked.png)](https://example.com)"
    );
}

/// The blank line and the line after it start as the paragraph's first line does, so the images stay in their list
/// item or quote.
#[test]
fn images_in_a_list_item_or_a_quote_stay_in_it() {
    let listed = images_apart("- The shots:\n\n  ![Before](before.png)\n  ![After](after.png)\n- Next");
    let quoted = images_apart("> ![Before](before.png)\n> ![After](after.png)");

    assert_eq!(listed, "- The shots:\n\n  ![Before](before.png)\n\n  ![After](after.png)\n- Next");
    assert_eq!(
        blocks(&listed),
        "List[ListItem[Paragraph[The shots:] Paragraph[Before] Paragraph[After]] ListItem[Paragraph[Next]]]"
    );
    assert_eq!(quoted, "> ![Before](before.png)\n>\n> ![After](after.png)");
    assert_eq!(blocks(&quoted), "Blockquote[Paragraph[Before] Paragraph[After]]");
}

#[test]
fn images_among_words_or_in_code_are_left_as_they_are() {
    for text in [
        "It went from ![Before](before.png)\nto ![After](after.png).",
        "```\n![Before](before.png)\n![After](after.png)\n```",
        "    ![Before](before.png)\n    ![After](after.png)",
        "![Only](only.png)\n\n![Apart](apart.png)",
        "| ![Before](before.png) | ![After](after.png) |\n| --- | --- |",
    ] {
        assert!(matches!(images_apart(text), Cow::Borrowed(_)), "{text:?} was changed");
    }
}

/// Only a text where an image may follow another is parsed, so a reply full of code, where Rust writes `vec![` and
/// `#![`, isn't parsed on every frame.
#[test]
fn only_a_text_with_an_image_after_another_is_parsed() {
    let parsed = [
        "![A](a.png)\n![B](b.png)",
        "> ![A](a.png)\n>\t![B](b.png)",
        "![A][a]  \r\n[![B](b.png)](https://example.com)",
        "![A](a.png)\\\n![B](b.png)",
    ];
    let not_parsed = [
        "let v = vec![1, 2];\n#![deny(warnings)]\nlet w = vec![v[0]];",
        "![Only](only.png)",
        "It went from ![A](a.png) to ![B](b.png).",
    ];

    for text in parsed {
        assert!(an_image_may_follow_another(text), "{text:?} isn't parsed");
    }
    for text in not_parsed {
        assert!(!an_image_may_follow_another(text), "{text:?} is parsed");
    }
}

/// The line after the blank one starts as the paragraph's first does, not as the image's did, which was code's.
#[test]
fn an_image_on_an_indented_line_is_not_made_code() {
    let apart = images_apart("![A](a.png)\n    ![B](b.png)");

    assert_eq!(apart, "![A](a.png)\n\n![B](b.png)");
    assert_eq!(blocks(&apart), "Paragraph[A] Paragraph[B]");
}

/// A line can go on a quote's or a list item's paragraph without starting as its first does, and the image on it stays
/// in the quote or the item.
#[test]
fn an_image_on_a_lazy_line_stays_in_its_quote_or_list_item() {
    let quoted = images_apart("> ![A](a.png)\n![B](b.png)");
    let listed = images_apart("- ![A](a.png)\n![B](b.png)");
    let numbered = images_apart("10) ![A](a.png)\n![B](b.png)");

    assert_eq!(quoted, "> ![A](a.png)\n>\n> ![B](b.png)");
    assert_eq!(blocks(&quoted), "Blockquote[Paragraph[A] Paragraph[B]]");
    assert_eq!(listed, "- ![A](a.png)\n\n  ![B](b.png)");
    assert_eq!(blocks(&listed), "List[ListItem[Paragraph[A] Paragraph[B]]]");
    assert_eq!(numbered, "10) ![A](a.png)\n\n    ![B](b.png)");
    assert_eq!(blocks(&numbered), "List[ListItem[Paragraph[A] Paragraph[B]]]");
}

#[test]
fn images_on_one_line_of_a_list_item_are_each_a_paragraph_in_it() {
    let apart = images_apart("- ![A](a.png) ![B](b.png)");

    assert_eq!(apart, "- ![A](a.png)\n\n  ![B](b.png)");
    assert_eq!(blocks(&apart), "List[ListItem[Paragraph[A] Paragraph[B]]]");
}

/// A footnote's lines after the first are four spaces in, however long its label, and a task's box is only on its
/// first, so a label or a box made spaces would put the image in code.
#[test]
fn images_in_a_footnote_or_a_task_stay_in_it() {
    let noted = images_apart("[^note]: ![A](a.png)\n![B](b.png)");
    let task = images_apart("1. [ ] ![A](a.png)\n   ![B](b.png)");
    let quoted_task = images_apart("> - [x] ![A](a.png) ![B](b.png)");

    assert_eq!(noted, "[^note]: ![A](a.png)\n\n    ![B](b.png)");
    assert_eq!(blocks(&noted), "FootnoteDefinition[Paragraph[A] Paragraph[B]]");
    assert_eq!(task, "1. [ ] ![A](a.png)\n\n   ![B](b.png)");
    assert_eq!(blocks(&task), "List[ListItem[Paragraph[A] Paragraph[B]]]");
    assert_eq!(quoted_task, "> - [x] ![A](a.png)\n>\n>   ![B](b.png)");
    assert_eq!(blocks(&quoted_task), "Blockquote[List[ListItem[Paragraph[A] Paragraph[B]]]]");
}

/// A tab is as wide as it takes to reach the next stop, and a list item can take only part of one.
#[test]
fn images_after_a_tab_stay_in_their_list_item() {
    let apart = images_apart("- The shots:\n\n\t![A](a.png) ![B](b.png)");

    assert_eq!(apart, "- The shots:\n\n\t![A](a.png)\n\n  ![B](b.png)");
    assert_eq!(blocks(&apart), "List[ListItem[Paragraph[The shots:] Paragraph[A] Paragraph[B]]]");
}
