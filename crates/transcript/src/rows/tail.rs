//! The end of the transcript: text the agent is still writing, the working line while it works, and a line for each
//! task it's running in the background.

use std::time::Duration;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, div, px,
};
use look::{Signal, TaskRing, Work};
use protocol::event::{AgentTask, TaskKind};
use protocol::tail::{BlockKind, Tail};

use super::line::{HEIGHT, still_icon};
use super::{agent_text, empty, row, thinking, turn_times};
use crate::item_key::ItemKey;
use crate::long_texts::LongTexts;
use crate::transcript::AtWork;

/// Text the agent is still writing, then the working line while it works, then what it's running in the background.
/// A thought shows its latest line, or all of it when `thinking_open`, the way it will once it's recorded. A call
/// shows once it's recorded: until then it isn't running, and the working line says the agent is at work.
pub(super) fn render_tail(
    tail: &Tail,
    at_work: &AtWork,
    texts: &LongTexts,
    thinking_open: bool,
    cx: &App,
) -> AnyElement {
    // A subagent's work shows in the call that started it, once it's recorded.
    let messages: Vec<_> = tail.messages.iter().filter(|m| m.parent_tool_use_id.is_none()).collect();
    if messages.is_empty() && !at_work.working && at_work.background.is_empty() {
        return empty();
    }
    let mut out = row();
    for message in messages {
        for block in &message.blocks {
            let key = ItemKey::Block(message.message_id.clone(), block.index);
            out = match block.kind {
                BlockKind::Text => out.child(agent_text(&key, &block.text, texts.get(&key))),
                BlockKind::Thinking if thinking_open => {
                    out.child(thought("Thinking…".to_string(), cx)).child(thinking::text(&key, &block.text, cx))
                }
                BlockKind::Thinking => out.child(thought(format!("Thinking… {}", last_line(&block.text)), cx)),
                BlockKind::ToolUse | BlockKind::Unknown => out,
            };
        }
    }
    out = out.when(at_work.working, |out| out.child(working_line(at_work.turn_started_at_ms, cx)));
    // Left going by a turn, or started during one.
    out.children(at_work.background.iter().map(|task| going(task, cx))).into_any_element()
}

/// The text the agent is still writing, each block with what it's known by.
pub(crate) fn texts_of(tail: &Tail) -> Vec<(ItemKey, &str)> {
    let messages = tail.messages.iter().filter(|m| m.parent_tool_use_id.is_none());
    let blocks = messages.flat_map(|m| m.blocks.iter().map(move |block| (m, block)));
    blocks
        .filter(|(_, block)| block.kind == BlockKind::Text)
        .map(|(m, block)| (ItemKey::Block(m.message_id.clone(), block.index), block.text.as_str()))
        .collect()
}

/// A line as tall as a call's, at body size, so nothing jumps when what's above it is recorded.
fn tail_line(cx: &App) -> Div {
    div().flex().items_center().gap_2().h(px(HEIGHT)).text_color(cx.theme().muted_foreground)
}

/// What a thought the agent is still writing says, after the icon its line will have once it's recorded. The working
/// line under it has the mark that says the agent is at work.
fn thought(text: String, cx: &App) -> Div {
    tail_line(cx).child(still_icon(thinking::ICON)).child(clipped(text))
}

/// A background task: its ring, and what the agent said it's for. The ring says it's in the background, so the words
/// don't, and pointing at the line does. The fan is the agent's, so no task has one.
fn going(task: &AgentTask, cx: &App) -> impl IntoElement + use<> {
    let (ring, what, pointed_at) = match task.kind {
        TaskKind::Command => (TaskRing::Running, "A command", "Running in the background"),
        TaskKind::Subagent => (TaskRing::Running, "A subagent", "Running in the background"),
        TaskKind::Monitor => (TaskRing::Watching, "A monitor", "Watching in the background"),
    };
    let text = task.description.clone().unwrap_or_else(|| what.to_string());
    tail_line(cx)
        .id(ElementId::Name(format!("background-{}", task.id).into()))
        .child(look::task_ring(ring, cx))
        .child(clipped(text))
        .tooltip(move |window, cx| Tooltip::element(move |_, _| said(pointed_at)).build(window, cx))
        .test_support()
}

/// What pointing at a background task's line says.
fn said(words: &'static str) -> impl IntoElement {
    div().id("background-pointed-at").aria_label(words).child(words).test_support()
}

fn clipped(text: String) -> impl IntoElement {
    div().overflow_hidden().whitespace_nowrap().text_ellipsis().child(text)
}

/// The mark with its light sweeping, and how long the turn has taken so far, in the code font so the digits stay put.
fn working_line(started_at_ms: Option<i64>, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let so_far = started_at_ms.map(|started| {
        let elapsed = look::now_ms(cx) - started;
        // The hub's clock can be a little ahead of this one.
        turn_times::so_far(Duration::from_millis(u64::try_from(elapsed).unwrap_or(0)))
    });
    tail_line(cx)
        .id("working-line")
        .font_family(theme.mono_font_family.clone())
        .text_size(theme.mono_font_size)
        .child(look::mark(Signal::Working, Work::NoPr, look::Size::Inline, cx))
        .children(so_far)
        .test_support()
}

fn last_line(text: &str) -> String {
    let line = text.lines().last().unwrap_or("");
    line.chars().rev().take(120).collect::<Vec<_>>().into_iter().rev().collect()
}
