//! What each row of the log looks like.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::questions;
use client_core::session_log::Row;
use gpui_kit::base::SelectableText;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, WeakEntity, Window, div, px,
};
use protocol::event::{
    ApprovalDecision, ApprovalOutcome, ContentBlock, EventBody, ExitReason, NoticeLevel, TurnOutcome,
};
use protocol::model::Seq;
use protocol::tail::{BlockKind, Tail};
use serde_json::Value;

use crate::log_block::{LogBlock, LogText};
use crate::{Model, Transcript};

const MONO: &str = "Menlo";
/// Above this, output goes through [`LogBlock`], which can hold anything but has no text selection yet.
const LOG_BLOCK_ABOVE: usize = 16 * 1024;

pub(crate) fn render_row(
    model: &Rc<RefCell<Model>>,
    this: &WeakEntity<Transcript>,
    ix: usize,
    cx: &mut App,
) -> AnyElement {
    let mut m = model.borrow_mut();
    if ix >= m.log.row_count() {
        let tail = m.log.tail().clone();
        drop(m);
        return render_tail(&tail, cx);
    }
    match m.log.row(ix) {
        Row::Event(event) => {
            let (seq, body) = (event.seq, event.body.clone());
            let (open, picks) = if let EventBody::ApprovalRequested { approval_id, .. } = &body {
                (m.log.open_approvals().iter().any(
                    |e| matches!(&e.body, EventBody::ApprovalRequested { approval_id: id, .. } if id == approval_id),
                ), m.picks.get(approval_id).cloned().unwrap_or_default())
            } else {
                (false, BTreeMap::default())
            };
            let expanded: Vec<usize> = m.expanded.iter().filter(|(s, _)| *s == seq).map(|(_, block)| *block).collect();
            drop(m);
            render_event(seq, &body, &expanded, open, &picks, this, cx)
        }
        Row::Pending(prompt) => {
            let text = prompt.text.clone();
            drop(m);
            prompt_bubble(ElementId::NamedInteger("pending".into(), ix as u64), text, true, cx)
        }
        Row::Unloaded => {
            m.focus = ix;
            if !m.fetch_scheduled {
                m.fetch_scheduled = true;
                let this = this.clone();
                cx.defer(move |cx| {
                    let _ = this.update(cx, Transcript::fetch_wanted);
                });
            }
            // A guess at the height. The list keeps its anchor when the real one arrives.
            div().h(px(120.)).into_any_element()
        }
    }
}

fn row() -> Div {
    v_flex().w_full().pb_3().gap_2()
}

fn empty() -> AnyElement {
    div().into_any_element()
}

fn prompt_bubble(id: ElementId, text: String, pending: bool, cx: &App) -> AnyElement {
    let theme = cx.theme();
    row()
        .child(
            div()
                .w_full()
                .px_3()
                .py_2()
                .rounded_lg()
                .bg(theme.secondary)
                .when(pending, |d| d.opacity(0.6))
                .child(TextView::markdown(id, SharedString::from(text)).selectable(true)),
        )
        .into_any_element()
}

fn muted_line(text: impl Into<SharedString>, cx: &App) -> AnyElement {
    row().child(div().text_xs().text_color(cx.theme().muted_foreground).child(text.into())).into_any_element()
}

fn turn_ended(outcome: TurnOutcome, duration_ms: u64, cost: f64, errors: &[String], cx: &App) -> AnyElement {
    let seconds = std::time::Duration::from_millis(duration_ms).as_secs_f64();
    match outcome {
        TurnOutcome::Cancelled => muted_line("Stopped", cx),
        TurnOutcome::Error => {
            let detail = errors.first().cloned().unwrap_or_else(|| "The turn failed".to_string());
            colored_line(detail, cx.theme().danger, cx)
        }
        TurnOutcome::Success | TurnOutcome::Unknown if cost >= 0.01 => {
            muted_line(format!("Finished in {seconds:.1}s · ${cost:.2}"), cx)
        }
        TurnOutcome::Success | TurnOutcome::Unknown => muted_line(format!("Finished in {seconds:.1}s"), cx),
    }
}

fn agent_exited(reason: ExitReason, stderr_tail: &[String], cx: &App) -> AnyElement {
    match reason {
        ExitReason::Requested => empty(),
        ExitReason::HubRestart => {
            muted_line("The app restarted while the agent was working. Send a message to continue.", cx)
        }
        ExitReason::WorkerLost | ExitReason::Unexpected | ExitReason::StartFailed | ExitReason::Unknown => {
            let detail = stderr_tail.last().cloned().unwrap_or_default();
            colored_line(format!("The agent stopped unexpectedly. {detail}"), cx.theme().danger, cx)
        }
    }
}

fn render_event(
    seq: Seq,
    body: &EventBody,
    expanded: &[usize],
    approval_open: bool,
    picks: &BTreeMap<String, Vec<String>>,
    this: &WeakEntity<Transcript>,
    cx: &App,
) -> AnyElement {
    match body {
        EventBody::ApprovalRequested { approval_id, tool_name, input, .. } if tool_name == questions::TOOL_NAME => {
            questions::card(seq, approval_id, input, approval_open, picks, this, cx)
        }
        EventBody::UserPrompt { text, .. } => {
            prompt_bubble(ElementId::NamedInteger("prompt".into(), seq), text.clone(), false, cx)
        }
        EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => {
            let rendered: Vec<AnyElement> = blocks
                .iter()
                .enumerate()
                .filter_map(|(b, block)| render_block(seq, b, block, expanded.contains(&b), this, cx))
                .collect();
            if rendered.is_empty() {
                return empty();
            }
            row().children(rendered).into_any_element()
        }
        EventBody::TurnEnded { outcome, duration_ms, cost_usd, errors, .. } => {
            turn_ended(*outcome, duration_ms.unwrap_or(0), cost_usd.unwrap_or(0.0), errors, cx)
        }
        EventBody::ApprovalRequested { approval_id, tool_name, input, description, .. } => {
            approval_card(seq, approval_id, tool_name, input, description.as_deref(), approval_open, this, cx)
        }
        EventBody::ApprovalResolved { outcome: ApprovalOutcome::Cancelled, .. } => muted_line("Approval cancelled", cx),
        EventBody::ApprovalResolved { outcome: ApprovalOutcome::Denied, .. } => muted_line("Denied", cx),
        EventBody::AgentMode { permission_mode } => {
            muted_line(format!("Approval mode is now {}", crate::mode_label(permission_mode)), cx)
        }
        EventBody::AgentExited { reason, stderr_tail, .. } => agent_exited(*reason, stderr_tail, cx),
        EventBody::Notice { level, message } => {
            let color = match level {
                NoticeLevel::Error => cx.theme().danger,
                NoticeLevel::Warning => cx.theme().warning,
                NoticeLevel::Info | NoticeLevel::Unknown => cx.theme().muted_foreground,
            };
            colored_line(message.clone(), color, cx)
        }
        // These tell the hub things, and there's nothing here for a person.
        EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentOther { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::Unknown => empty(),
    }
}

fn colored_line(text: String, color: Hsla, _: &App) -> AnyElement {
    row().child(div().text_sm().text_color(color).child(text)).into_any_element()
}

fn render_block(
    seq: Seq,
    b: usize,
    block: &ContentBlock,
    expanded: bool,
    this: &WeakEntity<Transcript>,
    cx: &App,
) -> Option<AnyElement> {
    let id = |kind: &'static str| ElementId::NamedInteger(kind.into(), (seq << 8) | b as u64);
    Some(match block {
        ContentBlock::Text { text } => {
            TextView::markdown(id("text"), SharedString::from(text.clone())).selectable(true).into_any_element()
        }
        ContentBlock::Thinking { text } if text.is_empty() => return None,
        ContentBlock::Thinking { text } => {
            disclosure(id("thinking"), "Thinking", first_line(text), expanded, seq, b, this, cx)
                .when(expanded, |d| {
                    d.child(div().pl_4().text_color(cx.theme().muted_foreground).child(
                        TextView::markdown(id("thinking-text"), SharedString::from(text.clone())).selectable(true),
                    ))
                })
                .into_any_element()
        }
        // These show as cards when they ask, with everything in them.
        ContentBlock::ToolUse { name, .. } if name == questions::TOOL_NAME || name == "ExitPlanMode" => return None,
        ContentBlock::ToolUse { name, input, .. } => h_flex()
            .gap_2()
            .text_xs()
            .child(div().flex_none().font_weight(FontWeight::SEMIBOLD).child(SharedString::from(name.clone())))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .font_family(MONO)
                    .text_color(cx.theme().muted_foreground)
                    .child(summarize_input(input)),
            )
            .into_any_element(),
        ContentBlock::ToolResult { is_error, text, .. } => {
            let label = if *is_error { "Error" } else { "Output" };
            let lines = text.lines().count();
            let summary = if lines == 1 { "1 line".to_string() } else { format!("{lines} lines") };
            disclosure(id("result"), label, summary, expanded, seq, b, this, cx)
                .when(*is_error, |d| d.text_color(cx.theme().danger))
                .when(expanded, |d| d.child(output(id("output"), text, *is_error, cx)))
                .into_any_element()
        }
        ContentBlock::Image { .. } => {
            div().text_xs().text_color(cx.theme().muted_foreground).child("Image").into_any_element()
        }
        ContentBlock::Unknown => return None,
    })
}

fn output(id: ElementId, text: &str, is_error: bool, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let color = if is_error { theme.danger } else { theme.muted_foreground };
    let frame = div()
        .ml_4()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(theme.secondary)
        .font_family(MONO)
        .text_size(px(12.))
        .text_color(color);
    if text.len() > LOG_BLOCK_ABOVE {
        frame.child(LogBlock { log: LogText::new(text), color, font: MONO.into() }).into_any_element()
    } else {
        frame.child(SelectableText::new(id, text.to_string())).into_any_element()
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "each is a separate input to the card, and a struct would only bundle them for this call"
)]
fn disclosure(
    id: ElementId,
    label: &str,
    summary: impl Into<SharedString>,
    expanded: bool,
    seq: Seq,
    b: usize,
    this: &WeakEntity<Transcript>,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let this = this.clone();
    let chevron = if expanded { "▾" } else { "▸" };
    v_flex().w_full().gap_1().text_xs().child(
        div()
            .id(id)
            .flex()
            .gap_2()
            .cursor_pointer()
            .text_color(theme.muted_foreground)
            .hover(|d| d.text_color(gpui_kit::white()))
            .on_click(move |_, _, cx| {
                let _ = this.update(cx, |t, cx| t.toggle(seq, b, cx));
            })
            .child(div().flex_none().child(chevron))
            .child(div().flex_none().font_weight(FontWeight::SEMIBOLD).child(SharedString::from(label.to_string())))
            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().child(summary.into())),
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "each is a separate input to the card, and a struct would only bundle them for this call"
)]
fn approval_card(
    seq: Seq,
    approval_id: &str,
    tool_name: &str,
    input: &Value,
    description: Option<&str>,
    open: bool,
    this: &WeakEntity<Transcript>,
    cx: &App,
) -> AnyElement {
    use gpui_kit::component::button::{Button, ButtonVariants as _};
    let theme = cx.theme();
    let summary = description.map_or_else(|| summarize_input(input).to_string(), str::to_string);
    let answer = |decision: ApprovalDecision| {
        let (this, approval_id) = (this.clone(), approval_id.to_string());
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            let _ = this.update(cx, |t, cx| t.answer_approval(approval_id.clone(), decision.clone(), cx));
        }
    };
    let allow = ApprovalDecision::Allow { updated_input: None, updated_permissions: None };
    let answer_button = |name: &'static str, label: &'static str, decision: ApprovalDecision| {
        Button::new(ElementId::NamedInteger(name.into(), seq)).label(label).on_click(answer(decision))
    };
    let (title, content, buttons): (String, AnyElement, Div) = if tool_name == "ExitPlanMode" {
        let plan = input.get("plan").and_then(Value::as_str).unwrap_or_default().to_string();
        let keep_planning = ApprovalDecision::Deny {
            message: "The user wants to keep planning. Ask them what to change.".to_string(),
            interrupt: false,
        };
        (
            "Plan".to_string(),
            TextView::markdown(ElementId::NamedInteger("plan".into(), seq), SharedString::from(plan))
                .selectable(true)
                .into_any_element(),
            h_flex()
                .gap_2()
                .child(answer_button("approve-plan", "Approve plan", allow).primary())
                .child(answer_button("keep-planning", "Keep planning", keep_planning)),
        )
    } else {
        let deny = ApprovalDecision::Deny { message: "The user denied this.".to_string(), interrupt: false };
        (
            format!("{tool_name} needs approval"),
            div().text_xs().font_family(MONO).text_color(theme.muted_foreground).child(summary).into_any_element(),
            h_flex()
                .gap_2()
                .child(answer_button("allow", "Allow", allow).primary())
                .child(answer_button("deny", "Deny", deny)),
        )
    };
    row()
        .child(
            v_flex()
                .w_full()
                .p_3()
                .gap_2()
                .rounded_lg()
                .border_1()
                .border_color(if open { theme.warning } else { theme.border })
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(title))
                .child(content)
                .when(open, |d| d.child(buttons)),
        )
        .into_any_element()
}

/// Text the agent is still writing. Tool input is hidden until it's complete, because it's partial JSON.
fn render_tail(tail: &Tail, cx: &App) -> AnyElement {
    if tail.is_empty() {
        return empty();
    }
    let mut out = row();
    for message in &tail.messages {
        for block in &message.blocks {
            let key = ((message.message_id.len() as u64) << 8) | u64::from(block.index);
            out = match block.kind {
                BlockKind::Text => {
                    let id = ElementId::NamedInteger("tail".into(), key);
                    out.child(TextView::markdown(id, SharedString::from(block.text.clone())).selectable(true))
                }
                BlockKind::Thinking => out.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("Thinking… {}", last_line(&block.text))),
                ),
                BlockKind::ToolUse => out.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{}…", block.tool_name.as_deref().unwrap_or("Tool"))),
                ),
                BlockKind::Unknown => out,
            };
        }
    }
    out.into_any_element()
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").chars().take(160).collect()
}

fn last_line(text: &str) -> String {
    let line = text.lines().last().unwrap_or("");
    line.chars().rev().take(120).collect::<Vec<_>>().into_iter().rev().collect()
}

/// One line saying what a tool is doing. Shows the field that matters for common tools.
pub(crate) fn summarize_input(input: &Value) -> SharedString {
    for key in ["command", "file_path", "path", "pattern", "url", "query", "description", "prompt"] {
        if let Some(value) = input.get(key).and_then(Value::as_str) {
            return first_line(value).into();
        }
    }
    first_line(&input.to_string()).into()
}

#[cfg(test)]
mod tests {
    use super::summarize_input;
    use serde_json::json;

    #[test]
    fn tool_summaries_pick_the_field_that_matters() {
        assert_eq!(summarize_input(&json!({"command": "cargo test\nsecond line", "description": "x"})), "cargo test");
        assert_eq!(summarize_input(&json!({"file_path": "src/a.rs", "old_string": "y"})), "src/a.rs");
        assert_eq!(summarize_input(&json!({"other": 1})), "{\"other\":1}");
    }
}
