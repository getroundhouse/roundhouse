//! Text the agent is still writing.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{AnyElement, App, Div, IntoElement, ParentElement, Styled, div, px};
use look::{Signal, Work};
use protocol::tail::{BlockKind, Tail};

use super::{agent_text, empty, row};
use crate::item_key::ItemKey;

/// Text the agent is still writing. Tool input is hidden until it's complete, because it's partial JSON.
pub(super) fn render_tail(tail: &Tail, cx: &App) -> AnyElement {
    if tail.is_empty() {
        return empty();
    }
    let mut out = row();
    for message in &tail.messages {
        for block in &message.blocks {
            out = match block.kind {
                BlockKind::Text => {
                    out.child(agent_text(&ItemKey::Block(message.message_id.clone(), block.index), &block.text))
                }
                BlockKind::Thinking => out.child(working(format!("Thinking… {}", last_line(&block.text)), cx)),
                BlockKind::ToolUse => {
                    out.child(working(format!("Running {}", block.tool_name.as_deref().unwrap_or("a tool")), cx))
                }
                BlockKind::Unknown => out,
            };
        }
    }
    out.into_any_element()
}

/// What the agent is doing, beside its mark with the light sweeping.
fn working(text: String, cx: &App) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .h(px(22.))
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(look::mark(Signal::Working, Work::NoPr, look::Size::Inline, cx))
        .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().child(text))
}

pub(super) fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").chars().take(160).collect()
}

fn last_line(text: &str) -> String {
    let line = text.lines().last().unwrap_or("");
    line.chars().rev().take(120).collect::<Vec<_>>().into_iter().rev().collect()
}
