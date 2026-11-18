//! A text cut into the blocks at its top, such as each paragraph, heading, list and table, so a document can draw only
//! those in sight, each as a text of its own. Each keeps what the whole text's parse says of it: where it stands in
//! the flow, which decides the gaps the kit leaves around it, and what it shows, for a copy before it's drawn. Each is
//! drawn from its own parse, which reads it as the whole text's does where the blocks are a blank line apart, as a
//! written document's are. Without one, the kit's parser reads a few lines by what's right above them, which a block
//! cut apart is read without, such as a line of `-` under a link's definition, which is words there and a rule alone,
//! as a text that goes on can be read otherwise than whole in the chat. What a block refers to elsewhere in the text, a
//! link's definition or a footnote's, it's parsed with, after its own text, and doesn't draw.

use std::ops::Range;
use std::sync::Arc;

use gpui_kit::base::text::editor_host::{BlockNode, FlowPosition};

use super::markdown::{ImagesAre, keyed, read};

/// One of the blocks at a text's top, as the whole text's parse has it.
pub(crate) struct TopBlock {
    /// What to parse it from: its bytes of the text, with the text's definitions after them if it may refer to one, so
    /// a reference link in it goes where the text says, and a footnote's mark reads as one, as they would in the whole
    /// text.
    pub(crate) source: String,
    /// How many bytes of `source` are its own, which it draws. What's after them is only read.
    pub(crate) shown: usize,
    /// Where it stands among the text's blocks, or `None` for a text that's one block, which stands alone.
    pub(crate) flow: Option<FlowPosition>,
    /// What each of its leaves shows, in order.
    pub(crate) leaves: Arc<[String]>,
}

/// `source`'s blocks at its top, but for those that draw nothing, such as a link's definition. One the kit's parser
/// can't read, whose blocks it doesn't say the place of, or that may define a footnote other than at its top, is one
/// block, the whole text, as it's drawn alone. A footnote's mark reads as one only where its definition is parsed with
/// it, and the kit draws one in a list or a quote as part of that, so it can't go with each block as one at the top
/// does.
pub(crate) fn top_blocks(source: &str) -> Vec<TopBlock> {
    cut(source, false)
}

/// `source`'s blocks at its top, as [`top_blocks`] cuts them, but with each list cut into its items, each a list of
/// its own, so what changed in a list can be told item by item. An ordered one's item keeps its number, which it's
/// written with, and items are as tight as the list's: no gap between them, which a list spread out has.
pub(crate) fn top_blocks_and_items(source: &str) -> Vec<TopBlock> {
    cut(source, true)
}

/// `source`'s blocks at its top, with each list cut into its items if `items`.
fn cut(source: &str, items: bool) -> Vec<TopBlock> {
    let whole = || {
        vec![TopBlock { source: source.to_string(), shown: source.len(), flow: None, leaves: super::leaves(source) }]
    };
    let Some(blocks) = read(source, 0, ImagesAre::TheKits) else { return whole() };
    // A code block from its line's start, since a fence indented by a few spaces takes as many off each of its lines.
    // Anything else from its first word, since an indented line can be words in a block that it would be code at the
    // start of.
    let from_line = |start: usize| source[..start].trim_end_matches([' ', '\t']).len();
    let from_word = |start: usize| source.len() - source[start..].trim_start_matches([' ', '\t']).len();
    let starts = |block: &BlockNode, start: usize| {
        if matches!(block, BlockNode::CodeBlock(_)) { from_line(start) } else { from_word(start) }
    };
    let spans = blocks.iter().map(|block| block.span().map(|span| starts(block, span.start)..span.end));
    let Some(spans) = spans.collect::<Option<Vec<_>>>() else { return whole() };
    if blocks.iter().any(|block| nests_a_footnote(block, source)) {
        return whole();
    }
    let definitions = definitions(&blocks, &spans, source);
    let all: Vec<&BlockNode> = blocks.iter().collect();
    let last = all.iter().rposition(|block| draws(block)).unwrap_or(0);
    let block = |block: &BlockNode, span: Range<usize>, flow: FlowPosition| {
        let mut leaves = Vec::new();
        keyed(block.clone(), &mut leaves);
        let own = &source[span];
        // Not after a code block, which refers to nothing, and one left open would take them in as code.
        let referred = if matches!(block, BlockNode::CodeBlock(_)) { "" } else { &referred_to(own, &definitions) };
        TopBlock { source: format!("{own}{referred}"), shown: own.len(), flow: Some(flow), leaves: leaves.into() }
    };
    (blocks.iter().zip(spans).enumerate())
        .filter(|(_, (block, _))| draws(block))
        .flat_map(|(ix, (top, span))| {
            let flow = FlowPosition::at(&all, ix, ix == last);
            match listed(top).filter(|_| items) {
                Some(listed) => {
                    let count = listed.len();
                    let item = |(at, (item, span)): (usize, (&BlockNode, Range<usize>))| {
                        let flow = if at == 0 { flow } else { flow.first(true) };
                        let flow = if at + 1 == count { flow } else { flow.last(true) };
                        block(item, span, flow)
                    };
                    listed.into_iter().enumerate().map(item).collect()
                }
                None => vec![block(top, span, flow)],
            }
        })
        .collect()
}

/// A list's items, each with its bytes of the text, if `block` is a list whose items all say where they are.
fn listed(block: &BlockNode) -> Option<Vec<(&BlockNode, Range<usize>)>> {
    let BlockNode::List { children, .. } = block else { return None };
    children.iter().map(|item| Some((item, item.span().map(|span| span.start..span.end)?))).collect()
}

/// Whether `block` holds a footnote's definition in a list or a quote, as a paragraph the kit's parser made of it.
fn nests_a_footnote(block: &BlockNode, source: &str) -> bool {
    let defines = |child: &BlockNode| {
        let span = child.span();
        let own = span.and_then(|span| source.get(span.start..span.end)).map(|own| own.trim_start_matches([' ', '\t']));
        matches!(child, BlockNode::Paragraph(_)) && own.is_some_and(defines_a_footnote)
    };
    match block {
        BlockNode::Root { children, .. }
        | BlockNode::Blockquote { children, .. }
        | BlockNode::Surface { children, .. }
        | BlockNode::List { children, .. }
        | BlockNode::ListItem { children, .. } => {
            children.iter().any(|child| defines(child) || nests_a_footnote(child, source))
        }
        // These hold no blocks, nor does a kind a later kit adds, as far as this knows.
        BlockNode::Paragraph(_)
        | BlockNode::Heading { .. }
        | BlockNode::CodeBlock(_)
        | BlockNode::Custom { .. }
        | BlockNode::Table(_)
        | BlockNode::Break { .. }
        | BlockNode::HorizontalRule { .. }
        | BlockNode::Definition { .. }
        | BlockNode::Unknown
        | _ => false,
    }
}

/// Whether the block at a text's top that's `block` defines a footnote, which the kit's parser makes a paragraph that
/// starts with its label.
fn defines_a_footnote(block: &str) -> bool {
    block.starts_with("[^") && block.find(']').is_some_and(|end| block[end + 1..].starts_with(':'))
}

/// Whether `block` draws anything, as a link's definition doesn't.
fn draws(block: &BlockNode) -> bool {
    !matches!(block, BlockNode::Definition { .. })
}

/// One of a text's definitions at its top, of a link or a footnote.
struct Definition<'a> {
    /// As it's written.
    written: &'a str,
    /// What a reference to it has in it, its label, as [`matched`] has it, or `None` if it isn't clear, so it goes
    /// with whatever may refer to one.
    label: Option<String>,
}

/// The text's definitions at its top, of links and of footnotes.
fn definitions<'a>(blocks: &[BlockNode], spans: &[Range<usize>], source: &'a str) -> Vec<Definition<'a>> {
    let spans = (blocks.iter().zip(spans)).map(|(block, span)| (block, &source[span.clone()]));
    let defined = spans.filter(|(block, written)| !draws(block) || defines_a_footnote(written));
    let label = |written: &str| written.find(']').map(|end| matched(&written[1..end]));
    defined.map(|(_, written)| Definition { written, label: label(written) }).collect()
}

/// The definitions `own` may refer to, by having one's label in it, such as `docs` in `[the docs][docs]` or `^1` in
/// a footnote's mark, each after a blank line, or nothing if it refers to none. Only those, since a text such as a
/// changelog can define hundreds, which each block would otherwise be parsed with. One it has the label of without
/// referring to it changes nothing it shows, so a label is looked for anywhere, which is simpler than finding each
/// reference as Markdown does.
fn referred_to(own: &str, definitions: &[Definition<'_>]) -> String {
    if !own.contains(']') {
        return String::new();
    }
    let own = matched(own);
    let referred =
        definitions.iter().filter(|definition| definition.label.as_ref().is_none_or(|label| own.contains(label)));
    referred.flat_map(|definition| ["\n\n", definition.written]).collect()
}

/// `text` as a label is matched: in lower case, with each run of white space one space, as Markdown matches a
/// reference to its definition. Markdown folds case by Unicode's rules, which differ from lower case for a few letters,
/// such as `ß`, whose references this misses.
fn matched(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}
