use crate::linked_images;
use gpui_kit::SharedString;
use gpui_kit::base::text::editor_host::{BlockNode, InlineNode, LinkMark, Paragraph, Table, parse_markdown};
use gpui_kit::component::text::MarkdownExtensions;
use std::collections::BTreeMap;
use std::sync::Arc;

/// What an image, or anything else drawn inline that isn't text, is in a leaf's text: one character, as the kit counts
/// it.
pub(crate) const ATOM: char = '\u{FFFC}';

/// A text as the kit parsed it, with its leaves keyed.
pub(crate) struct Parsed {
    /// What was parsed: the text as written, with its images apart.
    pub(in super::super) source: String,
    pub(in super::super) blocks: Vec<BlockNode>,
    /// What each leaf shows, by its key, which is its place in the text. A selection's bytes, and find's matches,
    /// are in these.
    pub(crate) leaves: Arc<[String]>,
    /// Where each reference link, such as `[the docs][1]`, goes, by what it's defined as. The kit's parser leaves
    /// these in the blocks for its text view to find, so a text drawn from its blocks is given them itself.
    pub(in super::super) links: BTreeMap<SharedString, LinkMark>,
    /// Where each block starts, to parse a text that goes on from it again from there, or `None` when it's to be
    /// parsed whole, as one that may define a link is, or one with a block the kit's parser didn't say the place of.
    pub(super) starts: Option<Vec<Start>>,
    /// Whether it was parsed whole, from its start, rather than joined from what it was parsed as before and what it
    /// went on to, which can differ, as [`again`] says.
    pub(in super::super) whole: bool,
    /// Whose its images are to draw, which it was parsed by.
    pub(in super::super) images: ImagesAre,
}

/// Whose a text's images are to draw, which is part of how it's parsed, so a text parsed for one isn't drawn as the
/// other. The transcript's own are those a reply points to, which `crate::linked_images` takes out of the kit's blocks
/// as the text is parsed, and draws so a click opens them. Either way a text has the same leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum ImagesAre {
    TheKits,
    TheTranscripts,
}

impl ImagesAre {
    /// Whose the images of `source` are to draw, where it's drawn with the images its row's event kept to look up, if
    /// `looked_up`: the transcript's, unless it's one `linked_images` would take nothing from, which is parsed the
    /// same either way. So a reply with no images keeps the parse it was written into, rather than being parsed again
    /// in the row of its record.
    pub(in super::super) fn of(source: &str, looked_up: bool) -> Self {
        if looked_up && linked_images::may_take(source) { Self::TheTranscripts } else { Self::TheKits }
    }

    /// What the kit parses a text with, from byte `from` of it on.
    pub(super) fn read_with(self, from: usize) -> Arc<MarkdownExtensions> {
        match self {
            Self::TheKits => Arc::default(),
            Self::TheTranscripts => linked_images::to_read(from),
        }
    }
}

/// Where a block starts: its byte in the text, and its first leaf's key.
#[derive(Clone, Copy, Debug)]
pub(super) struct Start {
    pub(super) byte: usize,
    pub(super) leaf: usize,
}

/// Parses `source`, whose images are `images` to draw, and keys each of its leaves by its place in it. One the kit's
/// parser can't read is a paragraph, as it's written.
pub(in super::super) fn parse(source: String, images: ImagesAre) -> Parsed {
    let (mut blocks, mut leaves, mut starts) = (Vec::new(), Vec::new(), Some(Vec::new()));
    if let Some(read) = read(&source, 0, images) {
        more(read, 0, &mut blocks, &mut leaves, &mut starts);
    } else {
        let mut paragraph = Paragraph::default();
        paragraph.push(InlineNode::new(source.clone()));
        blocks.push(BlockNode::Paragraph(leaf(paragraph, &mut leaves)));
        starts = None;
    }
    if may_define(&source) {
        starts = None;
    }
    made(source, blocks, leaves, starts, true, images)
}

/// Where a text that goes on from one parsed is parsed again from: the byte its block starts at, which block that is,
/// and its first leaf's key.
#[derive(Clone, Copy, Debug)]
pub(in super::super) struct Again {
    pub(in super::super) byte: usize,
    pub(super) block: usize,
    pub(super) leaf: usize,
}

/// Where `source` is parsed again from, if it goes on from what `parsed` was parsed from: the line its last block
/// starts on that has a blank line before it, and no list just before. The blocks before it are kept. So a block the
/// new text continues, such as a list or a fenced code block, isn't split in two, as the kit's text view parses a text
/// it's given more of. Nor is one that ended a paragraph by starting under it, such as a table or a heading, which more
/// can show isn't one, as a table's second line does, and then it's part of the paragraph again. Nor is a list, which
/// takes in a block after a blank line that more shows is another of its items. It's parsed from the line's start, not
/// the block's, since a fence indented by a few spaces takes as many off each of its lines.
///
/// The kit's parser reads a few things in a text by what's far before them, which none of this follows, such as a
/// table after a quote's lazy line, so what a text that goes on is parsed as can differ from a whole parse, as it can
/// in the kit's text view.
pub(in super::super) fn again(parsed: &Parsed, source: &str) -> Option<Again> {
    let starts = parsed.starts.as_ref()?;
    if source.len() <= parsed.source.len() || !source.starts_with(&parsed.source) {
        return None;
    }
    let after_list = |block: usize| block > 0 && matches!(parsed.blocks[block - 1], BlockNode::List { .. });
    let block = (0..starts.len()).rev().find(|&block| apart(source, starts[block].byte) && !after_list(block))?;
    let Start { byte, leaf } = starts[block];
    let byte = source[..byte].trim_end_matches([' ', '\t']).len();
    (!may_define(&source[byte..])).then_some(Again { byte, block, leaf })
}

/// `source`, which goes on from `parsed` as `again` says, parsed again from there.
pub(in super::super) fn gone_on(parsed: &Parsed, again: Again, source: String) -> Parsed {
    match rest(&source, again, parsed.images) {
        Some(rest) => joined(parsed, again, source, rest),
        None => parse(source, parsed.images),
    }
}

/// The blocks of `source` from where `again` says it's parsed again from on, whose images are `images` to draw, as
/// [`read`] reads them, if it can.
pub(in super::super) fn rest(source: &str, again: Again, images: ImagesAre) -> Option<Vec<BlockNode>> {
    read(&source[again.byte..], again.byte, images)
}

/// `source`, which goes on from `parsed` as `again` says, with `rest`, its blocks from there on, as [`rest`] read them.
pub(in super::super) fn joined(parsed: &Parsed, again: Again, source: String, rest: Vec<BlockNode>) -> Parsed {
    let Again { byte, block, leaf } = again;
    let (mut blocks, mut leaves) = (parsed.blocks[..block].to_vec(), parsed.leaves[..leaf].to_vec());
    let mut starts = parsed.starts.as_ref().map(|starts| starts[..block].to_vec());
    more(rest, byte, &mut blocks, &mut leaves, &mut starts);
    // Parsed again from its start, it's parsed whole.
    made(source, blocks, leaves, starts, byte == 0, parsed.images)
}

/// Whether the block that starts at `byte` of `source` has a blank line before it, or nothing, so no block before it
/// ended there because it started.
fn apart(source: &str, byte: usize) -> bool {
    fn blank_end(text: &str) -> &str {
        text.trim_end_matches([' ', '\t', '\r'])
    }
    let before = blank_end(&source[..byte]);
    before.strip_suffix('\n').map_or(before.is_empty(), |above| {
        let above = blank_end(above);
        above.is_empty() || above.ends_with('\n')
    })
}

/// Whether `text` may define a link, such as `[docs]: https://example.com`, or a footnote, which decides how a
/// reference to it is read, before it as well as after, so a text with one is parsed whole. A definition starts a line,
/// after any indent, quote or list marker, with `[`, and its label ends with `]:`, on that line or a later one. Code
/// such as TypeScript's `[key: string]: T` looks like one too, which costs only the time a whole parse takes.
fn may_define(text: &str) -> bool {
    let marker = |c: char| c.is_whitespace() || c.is_ascii_digit() || matches!(c, '>' | '-' | '*' | '+' | '.' | ')');
    text.contains("]:") && text.lines().any(|line| line.trim_start_matches(marker).starts_with('['))
}

/// Keys `parsed`, parsed from byte `from` of the text on, after `blocks` and their `leaves`, and says where each of
/// its blocks starts, unless one has no place.
fn more(
    parsed: Vec<BlockNode>,
    from: usize,
    blocks: &mut Vec<BlockNode>,
    leaves: &mut Vec<String>,
    starts: &mut Option<Vec<Start>>,
) {
    for block in parsed {
        let start = block.span().map(|span| Start { byte: from + span.start, leaf: leaves.len() });
        match (starts.as_mut(), start) {
            (Some(starts), Some(start)) => starts.push(start),
            (Some(_), None) => *starts = None,
            (None, _) => {}
        }
        blocks.push(keyed(block, leaves));
    }
}

fn made(
    source: String,
    blocks: Vec<BlockNode>,
    leaves: Vec<String>,
    starts: Option<Vec<Start>>,
    whole: bool,
    images: ImagesAre,
) -> Parsed {
    let mut links = BTreeMap::new();
    defined(&blocks, &mut links);
    Parsed { source, blocks, leaves: leaves.into(), links, starts, whole, images }
}

/// `source`'s blocks, as the kit's parser reads them, if it can, on any thread, where `source` starts at byte `from` of
/// the text it's the end of, and its images are `images` to draw. The `markdown` crate it parses with
/// panics on some texts, such as `"a\n===\n---\nb\n==="`, which the agent can write, and which would end the app, so a
/// text it panics on is one it can't read. Rust's default hook still says so on stderr each time, which a short text
/// drawn again after its blocks were put away, or counted by find, is.
pub(in super::super) fn read(source: &str, from: usize, images: ImagesAre) -> Option<Vec<BlockNode>> {
    std::panic::catch_unwind(|| parse_markdown(source, images.read_with(from))).ok()?.ok()
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
    parse(source.to_owned(), ImagesAre::TheKits).leaves
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
