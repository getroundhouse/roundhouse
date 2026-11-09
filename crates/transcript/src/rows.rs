//! What each row of the log looks like.

use std::cell::RefCell;
use std::rc::Rc;

use client_core::OpenSession;
use client_core::session_log::{PendingPrompt, Row};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, Div, Hsla, IntoElement, ParentElement, SharedString, Styled, WeakEntity, div, px};
use protocol::event::{ApprovalOutcome, ContentBlock, EventBody, ExitReason, NoticeLevel, SessionEvent, TurnOutcome};
use protocol::model::Seq;
use protocol::tail::Tail;

use crate::attachments::{self, Carried};
use crate::item_key::ItemKey;
use crate::linked_images::LinkedImages;
use crate::transcript::{Model, Transcript};
use crate::{calls, questions};

mod approval;
mod around;
mod line;
mod prompt;
mod tail;
mod thinking;
mod tools;

use approval::approval_card;
pub(crate) use around::SubagentCalls;
use around::{Around, BlockAround, drawn_in_its_call};
use prompt::{Progress, prompt_bubble};
use tail::render_tail;
use tools::Toggle;

/// The reading column. Wide windows keep lines short enough to read, and the column stays centred.
const COLUMN: f32 = 848.0;
/// How far the composer reaches past the column on each side, so the transcript's text sits inside its edges.
const COMPOSER_REACH: f32 = 24.0;

/// What's drawn under the transcript in its column, such as a note, so it starts where the rows do in a pane of any
/// width. It's inset as the scroller insets each row, and then as the row is.
pub fn in_column(content: impl IntoElement) -> Div {
    div().w_full().px_3().child(column(content))
}

/// The composer under the transcript, centred under the column and wider than it on each side. In a pane too narrow
/// for both, it keeps its own margin from the pane's edges, so it reaches less far past the column.
pub fn around_column(content: impl IntoElement) -> Div {
    let width = px(COLUMN + 2. * COMPOSER_REACH);
    div().w_full().px_4().flex().justify_center().child(div().w_full().max_w(width).child(content))
}

fn column(content: impl IntoElement) -> Div {
    div().w_full().px_4().flex().justify_center().child(div().w_full().max_w(px(COLUMN)).child(content))
}

pub(crate) fn render_row(
    model: &Rc<RefCell<Model>>,
    session: &OpenSession,
    this: &WeakEntity<Transcript>,
    linked: &LinkedImages,
    ix: usize,
    cx: &mut App,
) -> AnyElement {
    let row = render_row_inner(model, session, this, linked, ix, cx);
    column(row).into_any_element()
}

/// What a row shows, copied out so the log isn't held while it's drawn.
#[expect(clippy::large_enum_variant, reason = "made for one row and matched straight away")]
enum Shown {
    Tail(Tail),
    Event(SessionEvent, Around),
    Pending(PendingPrompt, Vec<Carried>),
    Unloaded,
    /// A subagent's event, which its call draws, so its own row is empty.
    InCall,
}

fn render_row_inner(
    model: &Rc<RefCell<Model>>,
    session: &OpenSession,
    this: &WeakEntity<Transcript>,
    linked: &LinkedImages,
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
                Row::Event(event) if drawn_in_its_call(log, &m, event) => Shown::InCall,
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
            render_event(event.seq, &event.body, &around, this, linked, cx)
        }
        Shown::Pending(prompt, carried) => {
            let carried = attachments::view(None, &carried, this, cx);
            prompt_bubble(&ItemKey::Prompt(prompt.client_msg_id), prompt.text, carried, Progress::Sending, cx)
        }
        Shown::InCall => empty(),
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

fn render_event(
    seq: Seq,
    body: &EventBody,
    around: &Around,
    this: &WeakEntity<Transcript>,
    linked: &LinkedImages,
    cx: &App,
) -> AnyElement {
    let approval_open = around.approval_open;
    match body {
        EventBody::ApprovalRequested { approval_id, tool_name, input, .. } if tool_name == questions::TOOL_NAME => {
            questions::card(seq, approval_id, input, approval_open, &around.picks, this, cx)
        }
        EventBody::UserPrompt { text, client_msg_id, .. } => {
            let carried = attachments::view(Some(seq), &around.carried, this, cx);
            let (key, progress) = (ItemKey::prompt(seq, client_msg_id.as_ref()), Progress::Recorded(around.prompt));
            prompt_bubble(&key, text.clone(), carried, progress, cx)
        }
        EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => {
            let rendered: Vec<AnyElement> = blocks
                .iter()
                .enumerate()
                .zip(&around.blocks)
                .filter_map(|((_, block), drawing)| render_block(seq, block, drawing, this, linked, cx))
                .collect();
            if rendered.is_empty() {
                return empty();
            }
            // Calls and thinking in a row read as one list, so they sit closer together than paragraphs do.
            let only_lines = blocks.iter().all(|b| {
                matches!(
                    b,
                    ContentBlock::ToolUse { .. } | ContentBlock::ToolResult { .. } | ContentBlock::Thinking { .. }
                )
            });
            row().when(only_lines, |d| d.pb_2().gap_2()).children(rendered).into_any_element()
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
        EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::AgentStarted { .. }
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
    linked: &LinkedImages,
    cx: &App,
) -> Option<AnyElement> {
    let (key, expanded) = (&drawing.key, drawing.open);
    let toggle = Toggle { seq, key, open: expanded, this, linked };
    Some(match block {
        ContentBlock::Text { text } => agent_text(key, text).image_source(linked.resolver(seq)).into_any_element(),
        ContentBlock::Thinking { text } if text.is_empty() => return None,
        ContentBlock::Thinking { text } => thinking::draw(text, &toggle, cx),
        // These show as cards when they ask, with everything in them.
        ContentBlock::ToolUse { name, .. } if calls::asks_in_a_card(name) => return None,
        ContentBlock::ToolUse { name, input, .. } => {
            tools::call(&toggle, name, input, drawing.answer.as_ref(), drawing.subagent.as_ref(), cx)
        }
        ContentBlock::ToolResult { .. } if drawing.with_call => return None,
        ContentBlock::ToolResult { is_error, text, .. } => tools::result(&toggle, *is_error, text, cx),
        ContentBlock::Image { .. } => {
            div().text_xs().text_color(cx.theme().muted_foreground).child("Image").into_any_element()
        }
        ContentBlock::Unknown => return None,
    })
}
