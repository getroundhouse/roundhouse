//! Text the agent is still writing.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::text::TextView;
use gpui_kit::{AnyElement, App, ElementId, IntoElement, ParentElement, SharedString, Styled, div};
use protocol::tail::{BlockKind, Tail};

use super::{empty, row};

/// SKETCH: the fan with its light sweeping, beside what the agent is doing.
fn working(text: String, cx: &App) -> gpui_kit::Div {
    gpui_kit::div()
        .flex()
        .items_center()
        .gap_2()
        .h(gpui_kit::px(22.))
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(crate::look::fan(crate::look::Signal::Working, crate::look::Work::NoPr, gpui_kit::px(16.), cx))
        .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().child(text))
}

/// Text the agent is still writing. Tool input is hidden until it's complete, because it's partial JSON.
pub(super) fn render_tail(tail: &Tail, cx: &App) -> AnyElement {
    if tail.is_empty() {
        return empty();
    }
    let mut out = row();
    for message in &tail.messages {
        for block in &message.blocks {
            let key = ((message.message_id.as_str().len() as u64) << 8) | u64::from(block.index);
            out = match block.kind {
                BlockKind::Text => {
                    let id = ElementId::NamedInteger("tail".into(), key);
                    out.child(TextView::markdown(id, SharedString::from(block.text.clone())).selectable(true))
                }
                BlockKind::Thinking => out.child(working(format!("Thinking  {}", last_line(&block.text)), cx)),
                BlockKind::ToolUse => {
                    out.child(working(format!("Running {}", block.tool_name.as_deref().unwrap_or("a tool")), cx))
                }
                BlockKind::Unknown => out,
            };
        }
    }
    out.into_any_element()
}

pub(super) fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").chars().take(160).collect()
}

fn last_line(text: &str) -> String {
    let line = text.lines().last().unwrap_or("");
    line.chars().rev().take(120).collect::<Vec<_>>().into_iter().rev().collect()
}
