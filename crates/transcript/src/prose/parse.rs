//! A text parsed by the kit's own Markdown parser, as its text view would, with each leaf keyed by its place in the
//! text: each paragraph, heading, code block and table cell. The kit reports where it drew each keyed leaf, and paints
//! the ranges it's given for one behind it.

use std::sync::Arc;

use gpui_kit::base::text::editor_host::{BlockNode, InlineNode, Paragraph, Table, parse_markdown};

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
}

/// Parses `source`, and keys each of its leaves by its place in it.
pub(super) fn parse(source: String) -> Parsed {
    let blocks = parse_markdown(&source, Arc::default()).unwrap_or_else(|_| {
        let mut paragraph = Paragraph::default();
        paragraph.push(InlineNode::new(source.clone()));
        vec![BlockNode::Paragraph(paragraph)]
    });
    let mut leaves = Vec::new();
    let blocks = blocks.into_iter().map(|block| keyed(block, &mut leaves)).collect();
    Parsed { source, blocks, leaves: leaves.into() }
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
    use super::{ATOM, leaves};

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
    fn a_link_is_its_words_and_not_its_address() {
        assert_eq!(*leaves("See [the docs](https://example.com) and `cargo test`."), ["See the docs and cargo test."]);
    }
}
