//! A message of the agent's, or what came back to it, drawn as its blocks: its words, its thinking, and its calls and
//! what came back for them, each where the row it's drawn in says.

use client_core::calls::Recorded;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement, Styled, div};
use look::Typeset as _;
use protocol::event::ContentBlock;
use protocol::model::Seq;

use super::around::{Around, BlockAround};
use super::tools::{self, Toggle};
use super::{Shared, empty, row, thinking, written};
use crate::find::{FindLook, Part};

/// A message of the agent's, or what came back to it, recorded at `seq`: each of its `blocks` as `around` says.
pub(super) fn draw(
    seq: Seq,
    blocks: &[ContentBlock],
    around: &Around,
    shared: &Shared,
    find: &FindLook,
    cx: &App,
) -> AnyElement {
    let rendered: Vec<AnyElement> = blocks
        .iter()
        .enumerate()
        .zip(&around.blocks)
        .filter_map(|((ix, block), drawing)| render_block(seq, InRow::Own(ix), block, drawing, shared, find, cx))
        .collect();
    if rendered.is_empty() {
        return empty();
    }
    // Calls and thinking in a row read as one list, so they sit closer together than paragraphs do.
    let only_lines = blocks.iter().all(is_a_line);
    row().when(only_lines, |d| d.pb(look::Space::Small).gap(look::Space::Small)).children(rendered).into_any_element()
}

/// A call, its result or thinking, which read as lines of one list.
fn is_a_line(block: &ContentBlock) -> bool {
    matches!(
        block,
        ContentBlock::ToolUse { .. }
            | ContentBlock::Call { .. }
            | ContentBlock::ToolResult { .. }
            | ContentBlock::CallResult { .. }
            | ContentBlock::Thinking { .. }
    )
}

/// Which row a block is drawn in: its event's own, as the `n`th of the event's blocks, or a call's, as a subagent's
/// words are, nested in the `n`th block of that row's event. Either way, `n` is the block its texts are placed in. The
/// questions the agent asked are each drawn as a block of their own, in their order, so each one's texts are together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::rows) enum InRow {
    Own(usize),
    Nested(usize),
}

impl InRow {
    /// The row what this block nests is drawn in: the same row, nested in the same block of its event, however deep.
    pub(in crate::rows) fn nesting(self) -> Self {
        match self {
            InRow::Own(block) | InRow::Nested(block) => InRow::Nested(block),
        }
    }
}

/// A block of the event at `seq`, drawn in the row `in_row` says.
pub(super) fn render_block(
    seq: Seq,
    in_row: InRow,
    block: &ContentBlock,
    drawing: &BlockAround,
    shared: &Shared,
    find: &FindLook,
    cx: &App,
) -> Option<AnyElement> {
    let key = &drawing.key;
    let (fold, open) = (drawing.fold, drawing.open);
    let toggle = Toggle { seq, key, fold, open, shared, find, in_row };
    Some(match block {
        ContentBlock::Text { text, images } => written(&toggle, Part::Text, text, None, Some(images)),
        ContentBlock::Thinking { text } if text.is_empty() => return None,
        ContentBlock::Thinking { text } => thinking::draw(text, &toggle, cx),
        ContentBlock::ToolUse { .. } | ContentBlock::Call { .. } => {
            // One that asks the person something is drawn from its approval, which has everything that was asked.
            let recorded = Recorded::of(block).filter(|call| !call.asks_the_person())?;
            tools::call(&toggle, recorded, drawing, cx)
        }
        ContentBlock::ToolResult { .. } | ContentBlock::CallResult { .. } if drawing.with_call => return None,
        ContentBlock::ToolResult { .. } | ContentBlock::CallResult { .. } => {
            tools::result(&toggle, drawing.reply.result()?, cx)
        }
        ContentBlock::Image { .. } => {
            div().typeset(look::Type::Detail).text_color(look::colors(cx).text_muted).child("Image").into_any_element()
        }
        ContentBlock::Unknown => return None,
    })
}
