//! Text the agent is still writing, and what it's running in the background.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement, Styled, TestSupportExt as _,
    div, px,
};
use look::{Signal, Work};
use protocol::event::AgentTask;
use protocol::tail::{BlockKind, Tail};

use super::{agent_text, empty, row, thinking_text};
use crate::item_key::ItemKey;

/// Text the agent is still writing, and what it's running in the background. Tool input is hidden until it's
/// complete, because it's partial JSON. Thinking shows its latest line, or all of it when `thinking_open`, the way it
/// will once it's recorded.
pub(super) fn render_tail(tail: &Tail, background: &[AgentTask], thinking_open: bool, cx: &App) -> AnyElement {
    if tail.is_empty() && background.is_empty() {
        return empty();
    }
    let mut out = row();
    for message in &tail.messages {
        for block in &message.blocks {
            let key = ItemKey::Block(message.message_id.clone(), block.index);
            out = match block.kind {
                BlockKind::Text => out.child(agent_text(&key, &block.text)),
                BlockKind::Thinking if thinking_open => {
                    out.child(working("Thinking…".to_string(), cx)).child(thinking_text(&key, &block.text, cx))
                }
                BlockKind::Thinking => out.child(working(format!("Thinking… {}", last_line(&block.text)), cx)),
                BlockKind::ToolUse => {
                    out.child(working(format!("Running {}", block.tool_name.as_deref().unwrap_or("a tool")), cx))
                }
                BlockKind::Unknown => out,
            };
        }
    }
    // Left going by a turn, or started during one, and still the agent's work.
    for task in background {
        let text = format!("In the background · {}", task.description.as_deref().unwrap_or("a task"));
        let id = ElementId::Name(format!("background-{}", task.id).into());
        out = out.child(working(text, cx).id(id).test_support());
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
