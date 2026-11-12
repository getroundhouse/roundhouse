//! The end of the transcript: text the agent is still writing, the working line while it works, and a line for each
//! task it's running in the background, which for a subagent has the robot and the button its call's line has.

use std::time::Duration;

use client_core::session_log::By;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, div, px,
};
use look::{Signal, TaskRing};
use protocol::event::{AgentTask, TaskKind};
use protocol::tail::{BlockKind, Compacting, Tail};

use super::line::{HEIGHT, still_icon};
use super::tools::agent::open_tab;
use super::{Shared, empty, row, text_id, thinking, turn_times};
use crate::calls::agent;
use crate::item_key::ItemKey;
use crate::transcript::AtWork;

/// Text the agent is still writing, then the working line while it works, then what it's running in the background.
/// A thought shows its latest line, or all of it when `thinking_open`, the way it will once it's recorded. A call
/// shows once it's recorded: until then it isn't running, and the working line says the agent is at work. A
/// subagent's line opens the subagent tab when the log is read `by` place, which keeps its work apart, since only
/// then can the tab read it.
pub(super) fn render_tail(
    tail: &Tail,
    at_work: &AtWork,
    shared: &Shared,
    by: By,
    thinking_open: bool,
    cx: &App,
) -> AnyElement {
    let texts = &shared.texts;
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
                BlockKind::Text => out.child(texts.streaming(text_id(&key), &block.text)),
                BlockKind::Thinking if thinking_open => {
                    let text = texts.streaming(thinking::text_id(&key), &block.text);
                    out.child(thought("Thinking…".to_string(), cx)).child(thinking::text(&key, text, cx))
                }
                BlockKind::Thinking => out.child(thought(format!("Thinking… {}", last_line(&block.text)), cx)),
                BlockKind::ToolUse | BlockKind::Unknown => out,
            };
        }
    }
    out = out
        .when(at_work.working, |out| out.child(working_line(at_work.turn_started_at_ms, tail.compacting.as_ref(), cx)));
    // Left going by a turn, or started during one.
    out.children(at_work.background.iter().map(|task| going(task, by, shared, cx))).into_any_element()
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

/// A background task: its ring, or a subagent's robot, and what the agent said it's for. The ring or the robot says
/// it's in the background, so the words don't, and pointing at them does. The fan is the agent's, so no task has one.
/// A subagent has the robot its call's line has, so its line reads as that call, and it ends with the button that
/// opens the subagent tab, as its call's line does, once the agent has said which call started it.
fn going(task: &AgentTask, by: By, shared: &Shared, cx: &App) -> impl IntoElement + use<> {
    let (lead, what, pointed_at) = match task.kind {
        TaskKind::Command => (ring(TaskRing::Running, cx), "A command", "Running in the background"),
        TaskKind::Subagent => (look::robot_at_work(cx).into_any_element(), "A subagent", "Running in the background"),
        TaskKind::Monitor => (ring(TaskRing::Watching, cx), "A monitor", "Watching in the background"),
    };
    let text = task.description.clone().unwrap_or_else(|| what.to_string());
    let open = task.call.as_ref().filter(|_| by == By::Place && task.kind == TaskKind::Subagent).map(|call| {
        let id = ElementId::Name(format!("background-open/{call}").into());
        open_tab(id, call.clone(), task.description.as_deref().unwrap_or(agent::UNTITLED), &shared.this)
    });
    // The button says what it does when pointed at, so the rest of the line says where the task runs, and gives way
    // to the button, which stays at the far end.
    let described = div()
        .id(ElementId::Name(format!("background-{}", task.id).into()))
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap_2()
        .child(lead)
        .child(clipped(text).id(ElementId::Name(format!("background-text/{}", task.id).into())).test_support())
        .tooltip(move |window, cx| Tooltip::element(move |_, _| said(pointed_at)).build(window, cx))
        .test_support();
    tail_line(cx).child(described).when_some(open, |line, open| line.child(div().flex_none().child(open)))
}

fn ring(ring: TaskRing, cx: &App) -> AnyElement {
    look::task_ring(ring, cx).into_any_element()
}

/// What pointing at a background task's line says.
fn said(words: &'static str) -> impl IntoElement {
    div().id("background-pointed-at").aria_label(words).child(words).test_support()
}

fn clipped(text: String) -> Div {
    div().overflow_hidden().whitespace_nowrap().text_ellipsis().child(text)
}

/// The mark with its light sweeping, and how long the turn has taken so far, in the code font so the digits stay put.
/// While the agent compacts its conversation, it says so after the time, and how much of the summary it has written
/// when it says, since that's the only sign a compaction is getting on.
fn working_line(started_at_ms: Option<i64>, compacting: Option<&Compacting>, cx: &App) -> impl IntoElement {
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
        .child(look::chat_mark(Signal::Working, look::Size::Inline, cx))
        .children(so_far)
        .children(compacting.map(|compacting| {
            // Counted as tokens are, since it's said beside them.
            let written = compacting.written_chars.filter(|chars| *chars > 0);
            let written = written.map(|chars| format!(" · {} characters written", look::tokens(chars)));
            // Cut off where the line ends, rather than running past the pane.
            div()
                .min_w_0()
                .font_family(theme.font_family.clone())
                .text_sm()
                .child(clipped(format!("Compacting the conversation{}", written.unwrap_or_default())))
        }))
        .test_support()
}

fn last_line(text: &str) -> String {
    let line = text.lines().last().unwrap_or("");
    line.chars().rev().take(120).collect::<Vec<_>>().into_iter().rev().collect()
}
