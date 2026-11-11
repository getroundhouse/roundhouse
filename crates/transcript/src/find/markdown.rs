//! Where the needle is in Markdown, as the chat shows it: in the text the kit's text view renders of it, so
//! `**parser** is` holds `parser is`, and a link's address holds nothing. Counting reads that text from the kit's own
//! parse, block by block, and a view finds each block in what it renders, so the two find the same matches.

use std::ops::Range;
use std::sync::Arc;

use gpui_kit::base::text::editor_host::{BlockNode, InlineNode, Paragraph, parse_markdown};
use protocol::find::Needle;

/// How many matches `source` shows, rendered.
pub(crate) fn count(needle: &Needle, source: &str) -> usize {
    if !needle.may_be_in_markdown(source) {
        return 0;
    }
    shown(source).iter().map(|text| needle.ranges(text).count()).sum()
}

/// Where the needle is in `rendered`, the text a view renders of `source`, in order. A match never runs from one
/// block into the next.
pub(crate) fn ranges(needle: &Needle, source: &str, rendered: &str) -> Vec<Range<usize>> {
    if !needle.may_be_in_markdown(source) {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut from = 0;
    for text in shown(source).iter().filter(|text| !text.is_empty()) {
        let Some(at) = rendered.get(from..).and_then(|rest| rest.find(text.as_str())) else { continue };
        let start = from + at;
        found.extend(needle.ranges(text).map(|range| start + range.start..start + range.end));
        from = start + text.len();
    }
    found
}

/// The text each block of `source` renders, in order: each paragraph, heading, code block and table cell.
fn shown(source: &str) -> Vec<String> {
    let Ok(blocks) = parse_markdown(source, Arc::default()) else { return vec![source.to_string()] };
    let mut texts = Vec::new();
    for block in &blocks {
        gather(block, &mut texts);
    }
    texts
}

fn gather(block: &BlockNode, texts: &mut Vec<String>) {
    // The kit's blocks can grow, and one it adds is drawn its own way, so what it shows isn't known here.
    #[expect(clippy::wildcard_enum_match_arm, reason = "the kit's blocks are non-exhaustive")]
    match block {
        BlockNode::Root { children, .. }
        | BlockNode::Blockquote { children, .. }
        | BlockNode::List { children, .. }
        | BlockNode::ListItem { children, .. } => {
            for child in children {
                gather(child, texts);
            }
        }
        BlockNode::Paragraph(paragraph) | BlockNode::Heading { children: paragraph, .. } => {
            texts.push(text_of(paragraph));
        }
        BlockNode::CodeBlock(code) => texts.push(code.code().to_string()),
        BlockNode::Table(table) => texts.extend(table.rows().flatten().map(text_of)),
        _ => {}
    }
}

fn text_of(paragraph: &Paragraph) -> String {
    paragraph.children().iter().map(InlineNode::text).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn needle(query: &str) -> Needle {
        Needle::new(query).unwrap()
    }

    #[test]
    fn markdown_is_found_as_it_renders() {
        assert_eq!(count(&needle("parser is"), "The **parser** is _fixed_"), 1);
        assert_eq!(count(&needle("run cargo"), "Run `cargo test`"), 1);
        assert_eq!(count(&needle("example.com"), "See [the docs](https://example.com)"), 0, "not a link's address");
        assert_eq!(count(&needle("**"), "**bold**"), 0, "nor what marks it");
        assert_eq!(count(&needle("cell"), "| a cell | another cell |\n|---|---|\n| cell | x |"), 3);
        assert_eq!(count(&needle("item"), "- an item\n- another item\n\n```\nitem\n```"), 3);
    }

    #[test]
    fn a_view_s_matches_are_those_counted_where_it_renders_them() {
        let source = "# The parser\n\nThe **parser** is _fixed_.";
        let rendered = "The parser\nThe parser is fixed.\n";

        let found = ranges(&needle("parser"), source, rendered);

        let texts: Vec<&str> = found.iter().map(|range| &rendered[range.clone()]).collect();
        assert_eq!(texts, ["parser", "parser"]);
        assert_eq!(found[1].start, "The parser\nThe ".len(), "in the second block");
        assert_eq!(found.len(), count(&needle("parser"), source));
    }
}
