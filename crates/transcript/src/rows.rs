//! What each row of the log looks like.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use client_core::session_log::Row;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, WeakEntity, div, px,
};
use protocol::event::{ApprovalOutcome, ContentBlock, EventBody, ExitReason, NoticeLevel, TurnOutcome};
use protocol::model::Seq;

use crate::questions;
use crate::transcript::{Model, Transcript};

mod approval;
mod tail;
mod tools;

use approval::approval_card;
use tail::{first_line, render_tail};
use tools::Answer;
pub(crate) use tools::summarize_input;

/// The reading column. Wide windows keep lines short enough to read, and the column stays centred.
const COLUMN: f32 = 780.0;

pub(crate) fn render_row(
    model: &Rc<RefCell<Model>>,
    this: &WeakEntity<Transcript>,
    ix: usize,
    cx: &mut App,
) -> AnyElement {
    let row = render_row_inner(model, this, ix, cx);
    div().w_full().px_4().flex().justify_center().child(div().w_full().max_w(px(COLUMN)).child(row)).into_any_element()
}

fn render_row_inner(model: &Rc<RefCell<Model>>, this: &WeakEntity<Transcript>, ix: usize, cx: &mut App) -> AnyElement {
    let mut m = model.borrow_mut();
    if ix >= m.log.row_count() {
        let tail = m.log.tail().clone();
        drop(m);
        return render_tail(&tail, cx);
    }
    match m.log.row(ix) {
        Row::Event(event) => {
            let (seq, body) = (event.seq, event.body.clone());
            let around = Around::read(&m, seq, &body);
            drop(m);
            render_event(seq, &body, &around, this, cx)
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

/// What drawing one event needs from the rest of the model, read while it's borrowed.
struct Around {
    /// The event's blocks the user has opened, by position.
    expanded: Vec<usize>,
    /// For an approval: whether it's still waiting, and what's been chosen on its questions so far.
    approval_open: bool,
    picks: BTreeMap<String, Vec<String>>,
    /// For a call: its result, by the call's position, to draw under it.
    answers: Vec<Option<Answer>>,
    /// For a result: whether it's drawn with its call instead, by the result's position.
    with_call: Vec<bool>,
}

impl Around {
    fn read(m: &Model, seq: Seq, body: &EventBody) -> Self {
        let expanded = m.expanded.iter().filter(|(s, _)| *s == seq).map(|(_, block)| *block).collect();
        let mut around = Around {
            expanded,
            approval_open: false,
            picks: BTreeMap::new(),
            answers: Vec::new(),
            with_call: Vec::new(),
        };
        if let EventBody::ApprovalRequested { approval_id, .. } = body {
            around.approval_open =
                m.log.open_approvals().iter().any(
                    |e| matches!(&e.body, EventBody::ApprovalRequested { approval_id: id, .. } if id == approval_id),
                );
            around.picks = m.picks.get(approval_id).cloned().unwrap_or_default();
        }
        if let EventBody::Assistant { blocks, .. } = body {
            around.answers = tools::answers(m, seq, blocks);
        }
        if let EventBody::AgentUser { blocks, .. } = body {
            around.with_call = blocks
                .iter()
                .map(|block| {
                    matches!(block, ContentBlock::ToolResult { tool_use_id, .. }
                        if tools::drawn_with_call(m, seq, tool_use_id))
                })
                .collect();
        }
        around
    }
}

pub(crate) fn row() -> Div {
    v_flex().w_full().pb_4().gap_2()
}

/// A card that can wait on the person, such as a question or an approval. While it waits, it has an amber edge and a
/// faint amber wash.
pub(crate) fn waiting_card(open: bool, cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .w_full()
        .p_3()
        .rounded_lg()
        .border_1()
        .border_color(if open { theme.warning.opacity(0.45) } else { theme.border })
        .when(open, |d| d.bg(theme.warning.opacity(0.05)))
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
                .px_3p5()
                .py_2p5()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.secondary)
                .when(pending, |d| d.opacity(0.6))
                .child(TextView::markdown(id, SharedString::from(text)).selectable(true)),
        )
        .into_any_element()
}

fn muted_line(text: impl Into<SharedString>) -> AnyElement {
    row().child(div().text_xs().text_color(look::faint()).child(text.into())).into_any_element()
}

fn turn_ended(outcome: TurnOutcome, duration_ms: u64, cost: f64, errors: &[String], cx: &App) -> AnyElement {
    let seconds = std::time::Duration::from_millis(duration_ms).as_secs_f64();
    match outcome {
        TurnOutcome::Cancelled => muted_line("Stopped"),
        TurnOutcome::Error => {
            let detail = errors.first().cloned().unwrap_or_else(|| "The turn failed".to_string());
            colored_line(detail, cx.theme().danger)
        }
        TurnOutcome::Success | TurnOutcome::Unknown if cost >= 0.01 => {
            muted_line(format!("Finished in {seconds:.1}s · ${cost:.2}"))
        }
        TurnOutcome::Success | TurnOutcome::Unknown => muted_line(format!("Finished in {seconds:.1}s")),
    }
}

fn agent_exited(reason: ExitReason, stderr_tail: &[String], cx: &App) -> AnyElement {
    match reason {
        ExitReason::Requested => empty(),
        ExitReason::HubRestart => {
            muted_line("The app restarted while the agent was working. Send a message to continue.")
        }
        ExitReason::WorkerLost | ExitReason::Unexpected | ExitReason::StartFailed | ExitReason::Unknown => {
            let detail = stderr_tail.last().cloned().unwrap_or_default();
            colored_line(format!("The agent stopped unexpectedly. {detail}"), cx.theme().danger)
        }
    }
}

fn render_event(seq: Seq, body: &EventBody, around: &Around, this: &WeakEntity<Transcript>, cx: &App) -> AnyElement {
    let approval_open = around.approval_open;
    match body {
        EventBody::ApprovalRequested { approval_id, tool_name, input, .. } if tool_name == questions::TOOL_NAME => {
            questions::card(seq, approval_id, input, approval_open, &around.picks, this, cx)
        }
        EventBody::UserPrompt { text, .. } => {
            prompt_bubble(ElementId::NamedInteger("prompt".into(), seq), text.clone(), false, cx)
        }
        EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => {
            let rendered: Vec<AnyElement> = blocks
                .iter()
                .enumerate()
                .filter_map(|(b, block)| render_block(seq, b, block, around, this, cx))
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
        EventBody::ApprovalResolved { outcome: ApprovalOutcome::Cancelled, .. } => muted_line("Approval cancelled"),
        EventBody::ApprovalResolved { outcome: ApprovalOutcome::Denied, .. } => muted_line("Denied"),
        EventBody::AgentMode { permission_mode } => {
            muted_line(format!("Approval mode is now {}", crate::mode_label(permission_mode)))
        }
        EventBody::AgentExited { reason, stderr_tail, .. } => agent_exited(*reason, stderr_tail, cx),
        EventBody::Notice { level, message } => {
            let color = match level {
                NoticeLevel::Error => cx.theme().danger,
                NoticeLevel::Warning => cx.theme().warning,
                NoticeLevel::Info | NoticeLevel::Unknown => cx.theme().muted_foreground,
            };
            colored_line(message.clone(), color)
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

fn colored_line(text: String, color: Hsla) -> AnyElement {
    row().child(div().text_sm().text_color(color).child(text)).into_any_element()
}

fn render_block(
    seq: Seq,
    b: usize,
    block: &ContentBlock,
    around: &Around,
    this: &WeakEntity<Transcript>,
    cx: &App,
) -> Option<AnyElement> {
    let id = |kind: &'static str| ElementId::NamedInteger(kind.into(), (seq << 8) | b as u64);
    let expanded = around.expanded.contains(&b);
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
        ContentBlock::ToolUse { name, input, .. } => {
            tools::call(name, input, around.answers.get(b).and_then(Option::as_ref), this, cx)
        }
        ContentBlock::ToolResult { .. } if around.with_call.get(b).copied().unwrap_or(false) => return None,
        ContentBlock::ToolResult { is_error, text, .. } => {
            tools::result(seq, b, *is_error, text, expanded, this, cx).into_any_element()
        }
        ContentBlock::Image { .. } => {
            div().text_xs().text_color(cx.theme().muted_foreground).child("Image").into_any_element()
        }
        ContentBlock::Unknown => return None,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "each is a separate input to the card, and a struct would only bundle them for this call"
)]
pub(super) fn disclosure(
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
    let chevron = if expanded { IconName::ChevronDown } else { IconName::ChevronRight };
    let (faint, foreground) = (look::faint(), theme.foreground);
    v_flex().w_full().gap_1().text_xs().child(
        div()
            .id(id)
            .flex()
            .items_center()
            .h(px(22.))
            .gap_1p5()
            .cursor_pointer()
            .text_color(faint)
            .hover(move |d| d.text_color(foreground))
            .on_click(move |_, _, cx| {
                let _ = this.update(cx, |t, cx| t.toggle(seq, b, cx));
            })
            .child(Icon::new(chevron).xsmall())
            .child(div().flex_none().font_weight(FontWeight::MEDIUM).child(SharedString::from(label.to_string())))
            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().child(summary.into())),
    )
}
