//! A text parsed by the kit's own Markdown parser, as its text view would, with each leaf keyed by its place in the
//! text: each paragraph, heading, code block and table cell. The kit reports where it drew each keyed leaf, and paints
//! the ranges it's given for one behind it.

use std::collections::BTreeMap;
use std::sync::Arc;

use gpui_kit::SharedString;
use gpui_kit::base::text::editor_host::{BlockNode, InlineNode, LinkMark, Paragraph, Table, parse_markdown};

/// What an image, or anything else drawn inline that isn't text, is in a leaf's text: one character, as the kit counts
/// it.
pub(crate) const ATOM: char = '\u{FFFC}';

/// A text as the kit parsed it, with its leaves keyed.
pub(crate) struct Parsed {
    /// What was parsed: the text as written, with its images apart.
    pub(super) source: String,
    pub(super) blocks: Vec<BlockNode>,
    /// What each leaf shows, by its key, which is its place in the text. A selection's bytes, and find's matches,
    /// are in these.
    pub(crate) leaves: Arc<[String]>,
    /// Where each reference link, such as `[the docs][1]`, goes, by what it's defined as. The kit's parser leaves
    /// these in the blocks for its text view to find, so a text drawn from its blocks is given them itself.
    pub(super) links: BTreeMap<SharedString, LinkMark>,
}

/// Parses `source`, and keys each of its leaves by its place in it. One the kit's parser can't read is a paragraph, as
/// it's written.
pub(super) fn parse(source: String) -> Parsed {
    let blocks = read(&source).unwrap_or_else(|| {
        let mut paragraph = Paragraph::default();
        paragraph.push(InlineNode::new(source.clone()));
        vec![BlockNode::Paragraph(paragraph)]
    });
    let mut leaves = Vec::new();
    let blocks: Vec<BlockNode> = blocks.into_iter().map(|block| keyed(block, &mut leaves)).collect();
    let mut links = BTreeMap::new();
    defined(&blocks, &mut links);
    Parsed { source, blocks, leaves: leaves.into(), links }
}

/// `source`'s blocks, as the kit's parser reads them, if it can. The `markdown` crate it parses with panics on some
/// texts, such as `"a\n===\n---\nb\n==="`, which the agent can write, and which would end the app, so a text it panics
/// on is one it can't read. Rust's default hook still says so on stderr each time, which a short text drawn again after
/// its blocks were put away, or counted by find, is.
fn read(source: &str) -> Option<Vec<BlockNode>> {
    std::panic::catch_unwind(|| parse_markdown(source, Arc::default())).ok()?.ok()
}

/// Adds where each reference link defined in `blocks` goes to `links`, unless it's there. As the Markdown standard has
/// it, the first definition of one is the one it goes by, wherever it is, even inside a quote or a list.
fn defined(blocks: &[BlockNode], links: &mut BTreeMap<SharedString, LinkMark>) {
    for block in blocks {
        match block {
            BlockNode::Definition { identifier, url, title, .. } => {
                let link = || LinkMark { url: url.clone(), identifier: Some(identifier.clone()), title: title.clone() };
                links.entry(identifier.clone()).or_insert_with(link);
            }
            BlockNode::Root { children, .. }
            | BlockNode::Blockquote { children, .. }
            | BlockNode::List { children, .. }
            | BlockNode::ListItem { children, .. } => defined(children, links),
            // These hold no blocks, nor does a kind a later kit adds, as far as this knows.
            BlockNode::Paragraph(_)
            | BlockNode::Heading { .. }
            | BlockNode::CodeBlock(_)
            | BlockNode::Custom { .. }
            | BlockNode::Table(_)
            | BlockNode::Break { .. }
            | BlockNode::HorizontalRule { .. }
            | BlockNode::Unknown
            | _ => {}
        }
    }
}

/// What each leaf of `source` shows, in order, as it's drawn.
pub(crate) fn leaves(source: &str) -> Arc<[String]> {
    parse(source.to_owned()).leaves
}

fn keyed(block: BlockNode, leaves: &mut Vec<String>) -> BlockNode {
    let all = |children: Vec<BlockNode>, leaves: &mut Vec<String>| -> Vec<BlockNode> {
        children.into_iter().map(|child| keyed(child, leaves)).collect()
    };
    match block {
        BlockNode::Root { children, span } => BlockNode::Root { children: all(children, leaves), span },
        BlockNode::Paragraph(paragraph) => BlockNode::Paragraph(leaf(paragraph, leaves)),
        BlockNode::Heading { level, children, span } => {
            BlockNode::Heading { level, children: leaf(children, leaves), span }
        }
        BlockNode::Blockquote { children, span } => BlockNode::Blockquote { children: all(children, leaves), span },
        BlockNode::List { children, ordered, start, span } => {
            BlockNode::List { children: all(children, leaves), ordered, start, span }
        }
        BlockNode::ListItem { children, spread, checked, span } => {
            BlockNode::ListItem { children: all(children, leaves), spread, checked, span }
        }
        BlockNode::CodeBlock(code) => {
            let key = next_key(leaves);
            leaves.push(code.code().to_string());
            BlockNode::CodeBlock(code.with_leaf_key(key))
        }
        // A cell can't be keyed in place, so the table is made again from its cells.
        BlockNode::Table(table) => {
            let aligns = table.column_aligns().to_vec();
            let rows = table.rows().map(|cells| cells.map(|cell| leaf(cell.clone(), leaves)).collect()).collect();
            BlockNode::Table(Table::new(rows, aligns))
        }
        // These have no text of their own to select, nor does a kind a later kit adds, since the kit's are
        // non-exhaustive, so they're drawn as they are.
        BlockNode::Custom { .. }
        | BlockNode::Break { .. }
        | BlockNode::HorizontalRule { .. }
        | BlockNode::Definition { .. }
        | BlockNode::Unknown
        | _ => block,
    }
}

/// `paragraph` as a leaf, unless it's only images, which have no text to select. The kit draws a keyed paragraph
/// with an image in it as a flow of text, which doesn't grow when an image comes in after it's first laid out, and
/// one that's only images as images, which do.
fn leaf(paragraph: Paragraph, leaves: &mut Vec<String>) -> Paragraph {
    let children = paragraph.children();
    if !children.is_empty() && children.iter().all(|node| node.as_image().is_some()) {
        return paragraph;
    }
    let atom = |node: &InlineNode| node.as_image().is_some() || node.as_custom().is_some();
    let text =
        children.iter().map(|node| if atom(node) { ATOM.to_string() } else { node.text().to_string() }).collect();
    let key = next_key(leaves);
    leaves.push(text);
    paragraph.with_leaf_key(key)
}

fn next_key(leaves: &[String]) -> u64 {
    u64::try_from(leaves.len()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::{ATOM, leaves, parse};

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
    fn a_reference_link_goes_where_its_first_definition_says_even_inside_a_quote() {
        let source =
            "See [the docs][docs].\n\n> [docs]: https://example.com/first\n\n[docs]: https://example.com/second";

        let links = parse(source.to_owned()).links;

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
}
