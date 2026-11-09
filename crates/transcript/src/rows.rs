//! What each row of the log looks like.

use std::cell::RefCell;
use std::rc::Rc;

use client_core::OpenSession;
use client_core::session_log::{PendingPrompt, Row};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, TestSupportExt as _, WeakEntity, div, px,
};
use protocol::event::{ApprovalOutcome, ContentBlock, EventBody, ExitReason, NoticeLevel, SessionEvent, TurnOutcome};
use protocol::model::Seq;
use protocol::tail::Tail;

use crate::attachments::{self, Carried};
use crate::item_key::ItemKey;
use crate::transcript::{Model, Transcript};
use crate::{calls, questions};

mod approval;
mod around;
mod prompt;
mod tail;
mod tools;

use approval::approval_card;
use around::{Around, BlockAround};
use prompt::prompt_bubble;
use tail::{first_line, render_tail};
use tools::Toggle;

/// The reading column. Wide windows keep lines short enough to read, and the column stays centred.
const COLUMN: f32 = 780.0;

pub(crate) fn render_row(
    model: &Rc<RefCell<Model>>,
    session: &OpenSession,
    this: &WeakEntity<Transcript>,
    ix: usize,
    cx: &mut App,
) -> AnyElement {
    let row = render_row_inner(model, session, this, ix, cx);
    div().w_full().px_4().flex().justify_center().child(div().w_full().max_w(px(COLUMN)).child(row)).into_any_element()
}

/// What a row shows, copied out so the log isn't held while it's drawn.
#[expect(clippy::large_enum_variant, reason = "made for one row and matched straight away")]
enum Shown {
    Tail(Tail),
    Event(SessionEvent, Around),
    Pending(PendingPrompt, Vec<Carried>),
    Unloaded,
}

fn render_row_inner(
    model: &Rc<RefCell<Model>>,
    session: &OpenSession,
    this: &WeakEntity<Transcript>,
    ix: usize,
    cx: &mut App,
) -> AnyElement {
    let shown = {
        let m = model.borrow();
        session.read(|log| {
            if ix >= log.row_count() {
                return Shown::Tail(log.tail().clone());
            }
            match log.row(ix) {
                Row::Event(event) => Shown::Event(event.clone(), Around::read(log, &m, event.seq, &event.body)),
                Row::Pending(prompt) => Shown::Pending(prompt.clone(), attachments::carried(&m, &prompt.attachments)),
                Row::Unloaded => Shown::Unloaded,
            }
        })
    };
    match shown {
        Shown::Tail(tail) => {
            let model = model.borrow();
            render_tail(&tail, &model.background, model.folds.thinking_open(), cx)
        }
        Shown::Event(event, around) => {
            attachments::fetch_missing(model, this, Some(event.seq), &around.carried, cx);
            render_event(event.seq, &event.body, &around, this, cx)
        }
        Shown::Pending(prompt, carried) => {
            let carried = attachments::view(None, &carried, this, cx);
            prompt_bubble(&ItemKey::Prompt(prompt.client_msg_id), prompt.text, carried, true, cx)
        }
        Shown::Unloaded => {
            session.want(ix);
            // A guess at the height. The list keeps its anchor when the real one arrives.
            div().h(px(120.)).into_any_element()
        }
    }
}

/// Text the agent wrote, under the same ID while it streams and once it's recorded.
fn agent_text(key: &ItemKey, text: &str) -> TextView {
    TextView::markdown(key.element("text"), SharedString::from(text.to_string())).selectable(true)
}

/// What the agent thought, opened, under the same ID while it streams and once it's recorded.
fn thinking_text(key: &ItemKey, text: &str, cx: &App) -> impl IntoElement {
    let text = TextView::markdown(key.element("thinking-text"), SharedString::from(text.to_string()));
    div()
        .id(key.element("thinking"))
        .pl_4()
        .text_color(cx.theme().muted_foreground)
        .child(text.selectable(true))
        .test_support()
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
        EventBody::UserPrompt { text, client_msg_id, .. } => {
            let carried = attachments::view(Some(seq), &around.carried, this, cx);
            prompt_bubble(&ItemKey::prompt(seq, client_msg_id.as_ref()), text.clone(), carried, false, cx)
        }
        EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => {
            let rendered: Vec<AnyElement> = blocks
                .iter()
                .enumerate()
                .zip(&around.blocks)
                .filter_map(|((_, block), drawing)| render_block(seq, block, drawing, this, cx))
                .collect();
            if rendered.is_empty() {
                return empty();
            }
            // Calls in a row read as one list, so they sit closer together than paragraphs do.
            let only_tools =
                blocks.iter().all(|b| matches!(b, ContentBlock::ToolUse { .. } | ContentBlock::ToolResult { .. }));
            row().when(only_tools, |d| d.pb_2().gap_2()).children(rendered).into_any_element()
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
        | EventBody::AgentTasks { .. }
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
    block: &ContentBlock,
    drawing: &BlockAround,
    this: &WeakEntity<Transcript>,
    cx: &App,
) -> Option<AnyElement> {
    let (key, expanded) = (&drawing.key, drawing.open);
    let toggle = Toggle { seq, key, open: expanded, this };
    Some(match block {
        ContentBlock::Text { text } => agent_text(key, text).into_any_element(),
        ContentBlock::Thinking { text } if text.is_empty() => return None,
        ContentBlock::Thinking { text } => disclosure("Thinking", first_line(text), expanded, seq, key, this, cx)
            .when(expanded, |d| d.child(thinking_text(key, text, cx)))
            .into_any_element(),
        // These show as cards when they ask, with everything in them.
        ContentBlock::ToolUse { name, .. } if calls::asks_in_a_card(name) => return None,
        ContentBlock::ToolUse { name, input, .. } => tools::call(&toggle, name, input, drawing.answer.as_ref(), cx),
        ContentBlock::ToolResult { .. } if drawing.with_call => return None,
        ContentBlock::ToolResult { is_error, text, .. } => tools::result(&toggle, *is_error, text, cx),
        ContentBlock::Image { .. } => {
            div().text_xs().text_color(cx.theme().muted_foreground).child("Image").into_any_element()
        }
        ContentBlock::Unknown => return None,
    })
}

/// A line that opens and closes the block `key`, drawn in the row of the event at `seq`.
pub(super) fn disclosure(
    label: &str,
    summary: impl Into<SharedString>,
    expanded: bool,
    seq: Seq,
    key: &ItemKey,
    this: &WeakEntity<Transcript>,
    cx: &App,
) -> Div {
    let theme = cx.theme();
    let id = key.element("disclosure");
    let (this, key) = (this.clone(), key.clone());
    let chevron = if expanded { IconName::ChevronDown } else { IconName::ChevronRight };
    let (faint, foreground) = (look::faint(), theme.foreground);
    let label = SharedString::from(label.to_string());
    v_flex().w_full().gap_1().text_xs().child(
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(label.clone())
            .aria_expanded(expanded)
            .flex()
            .items_center()
            .h(px(22.))
            .gap_1p5()
            .cursor_pointer()
            .text_color(faint)
            .hover(move |d| d.text_color(foreground))
            .on_click(move |_, _, cx| {
                let _ = this.update(cx, |t, cx| t.set_open(seq, key.clone(), !expanded, cx));
            })
            .child(Icon::new(chevron).xsmall())
            .child(div().flex_none().font_weight(FontWeight::MEDIUM).child(label))
            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().child(summary.into()))
            .test_support(),
    )
}
