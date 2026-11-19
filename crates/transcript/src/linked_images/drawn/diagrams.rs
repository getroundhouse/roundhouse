//! How a text drawn from the kit's blocks draws the Mermaid diagrams written in it: each fenced code block written in
//! `mermaid` is taken as the kit parses the text, and drawn as an image alone on its line is, from what
//! `crate::diagrams` draws of it, so a click opens it over the window. One still being written, whose fence isn't
//! closed, is left to the kit, as a code block, since it can't be drawn till it's all there. While it's drawn on
//! another thread, it's nothing, as an image is while it loads, and one that can't be drawn is the code block it was
//! written as.
//!
//! A diagram is a leaf of its text, holding its code, as the code block it is in a text parsed for the kit, so a text
//! has the same leaves either way. A selection copies its code, and find finds what it says, as for a code block.

use super::super::LinkedImages;
use super::super::loading::{Source, Waiter};
use super::placement::Reading;
use crate::diagrams::{self, Palette};
use crate::images_that_open;
use gpui_kit::base::text::editor_host::{BlockNode, CodeBlock, FlowPosition};
use gpui_kit::component::text::markdown_ast::Node;
use gpui_kit::component::text::{MarkdownNode, MarkdownParseContext};
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement as _, ParentElement as _, TestSupportExt as _,
    Window, div,
};
use parking_lot::Mutex;
use std::sync::Arc;

/// The name the kit knows a diagram's node by.
pub(super) const DIAGRAM: &str = "mermaid-diagram";

/// A diagram written in a text: what it says, where it starts in the text, which no other block in the text shares, so
/// it names the diagram's element, and which leaf of the text it is, once its leaves are keyed.
pub(crate) struct Diagram {
    text: Arc<str>,
    at: usize,
    leaf: Mutex<Option<u64>>,
}

impl Diagram {
    fn element(&self) -> ElementId {
        ElementId::Name(format!("diagram/{}", self.at).into())
    }
}

/// What a diagram's leaf holds, if `node` is a diagram, which it's then keyed as `key`.
pub(crate) fn diagram_leaf(node: &MarkdownNode, key: u64) -> Option<String> {
    let diagram = node.data::<Diagram>()?;
    *diagram.leaf.lock() = Some(key);
    Some(diagram.text.to_string())
}

impl Reading {
    /// Takes a fenced code block written in `mermaid`, once its fence is closed.
    pub(super) fn diagram(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Code(code) = node else { return None };
        if !diagrams::is_diagram(code.lang.as_deref()) {
            return None;
        }
        let written = cx.node_source(node).filter(|written| closed(written))?;
        let at = self.from + cx.offset() + code.position.as_ref()?.start.offset;
        let diagram = Diagram { text: code.value.as_str().into(), at, leaf: Mutex::default() };
        Some(MarkdownNode::new(DIAGRAM, diagram).text(code.value.clone()).markdown(written.to_string()))
    }
}

/// Whether a fenced code block written as `written` is closed: a line after its first that's only the character its
/// fence is made of, as many times as the fence has it or more, as `CommonMark` closes one. In a quote, each line after
/// the first starts with the quote's marks, which aren't the block's.
fn closed(written: &str) -> bool {
    let mut lines = written.lines();
    let Some(opening) = lines.next().map(|line| line.trim_start_matches(' ')) else { return false };
    let Some(fence) = opening.chars().next().filter(|fence| matches!(fence, '`' | '~')) else { return false };
    let long = opening.chars().take_while(|c| *c == fence).count();
    let closing = lines.last().map(|line| line.trim_start_matches(|c: char| c == '>' || c.is_whitespace()).trim_end());
    closing.is_some_and(|closing| closing.len() >= long && closing.chars().all(|c| c == fence))
}

impl<W: Waiter> LinkedImages<W> {
    /// The diagram `node`, as wide as it's drawn, up to the column's, or nothing while it's drawn, or its code block,
    /// if it can't be drawn.
    pub(super) fn diagram(&self, node: &MarkdownNode, waiter: W, window: &mut Window, cx: &mut App) -> AnyElement {
        let Some(diagram) = node.data::<Diagram>() else { return div().into_any_element() };
        let source = Source::Diagram(diagram.text.clone(), Palette::of(cx));
        match self.drawn(&source, waiter, cx) {
            Some(Ok((image, covers))) => images_that_open::whole(div().id(diagram.element()), image, covers),
            Some(Err(_)) => as_code(diagram, window, cx),
            None => div().into_any_element(),
        }
    }
}

/// `diagram` as the code block it was written as, its text's leaf, as the kit would draw it in a text parsed for it.
fn as_code(diagram: &Diagram, window: &mut Window, cx: &mut App) -> AnyElement {
    let mut code = CodeBlock::from_code(diagram.text.clone(), Some(diagrams::LANGUAGE));
    if let Some(leaf) = *diagram.leaf.lock() {
        code = code.with_leaf_key(leaf);
    }
    let block = BlockNode::CodeBlock(code);
    let at = FlowPosition::at(&[&block], 0, true);
    let drawn = block.render_with_id("code", at, &crate::texts::code_context(cx), window, cx);
    div().id(diagram.element()).child(drawn).test_support().into_any_element()
}
