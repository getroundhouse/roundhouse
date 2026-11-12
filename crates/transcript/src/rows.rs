//! What each row of the log looks like.

use std::cell::RefCell;
use std::rc::Rc;

use client_core::OpenSession;
use client_core::session_log::{By, PendingPrompt, Row};
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, Hsla, IntoElement, ParentElement, Pixels, SharedString, Styled, WeakEntity, div,
    px,
};
use look::LineText;
use protocol::event::{ApprovalOutcome, ContentBlock, EventBody, ExitReason, NoticeLevel, SessionEvent};
use protocol::model::Seq;
use protocol::tail::Tail;

use crate::attachments::{self, Carried};
use crate::find::{self, FindLook, Marks, Part};
use crate::item_key::ItemKey;
use crate::linked_images::LinkedImages;
use crate::long_texts::LongTexts;
use crate::transcript::{Model, Transcript};
use crate::{calls, questions};

mod alone;
mod approval;
mod around;
mod asked;
mod compaction;
mod line;
mod mode;
mod prompt;
mod shown;
mod tail;
mod thinking;
mod tools;
mod turn_end;
mod turn_times;

pub(crate) use alone::rows_repeating_report;
pub use alone::subagent_standing;
pub(crate) use around::{NEAR, REACH};
pub(crate) use shown::{Found, of_row as found_in_row, rows_drawing, rows_to_count};

use approval::approval_card;
pub(crate) use around::SubagentCalls;
use around::{Around, BlockAround, drawn_in_its_call};
use prompt::{Progress, Prompt, prompt_bubble};
use tail::render_tail;
use tools::Toggle;

/// The reading column. Wide windows keep lines short enough to read, and the column stays centred.
const COLUMN: f32 = 848.0;
/// How far the composer reaches past the column on each side, so the transcript's text sits inside its edges.
const COMPOSER_REACH: f32 = 24.0;
/// The room the composer's row leaves between the chat's rows and the composer, or a question in its place.
pub const ABOVE_THE_COMPOSER: Pixels = px(4.);

/// What's left between the list and what's under it: the composer's room in a chat, and nothing in a subagent's tab,
/// which has no composer, since a subagent can't be prompted.
pub(crate) fn room_under(m: &Model) -> Pixels {
    if m.alone.is_some() { Pixels::ZERO } else { ABOVE_THE_COMPOSER }
}

/// What's drawn under the transcript in its column, such as a note, so it starts where the rows do in a pane of any
/// width. It's inset as the list insets each row, and then as the row is.
pub fn in_column(content: impl IntoElement) -> Div {
    div().w_full().px_3().child(column(content))
}

/// As wide as the composer under the transcript, which is centred under the column and wider than it on each side, for
/// the composer, and for the list's button back to its latest row, which sits on the composer's left edge. In a pane
/// too narrow for both, it keeps its own margin from the pane's edges, so it reaches less far past the column.
pub fn around_column(content: impl IntoElement) -> Div {
    let width = px(COLUMN + 2. * COMPOSER_REACH);
    div().w_full().px_4().flex().justify_center().child(div().w_full().max_w(width).child(content))
}

fn column(content: impl IntoElement) -> Div {
    div().w_full().px_4().flex().justify_center().child(div().w_full().max_w(px(COLUMN)).child(content))
}

/// Which of the list's items is drawn, and which is the tail, the list's last.
#[derive(Clone, Copy)]
pub(crate) struct Item {
    pub(crate) ix: usize,
    pub(crate) tail: usize,
}

/// What every row is drawn with: the transcript it's in, the images its text can point to, and where each kind of
/// text on a line goes. Made once for each frame, since it's the same for every row.
pub(crate) struct Shared {
    pub(crate) this: WeakEntity<Transcript>,
    pub(crate) linked: LinkedImages,
    pub(crate) line_text: LineText,
    /// Where every text view the rows draw is made.
    pub(crate) texts: LongTexts,
}

pub(crate) fn render_row(
    model: &Rc<RefCell<Model>>,
    session: &OpenSession,
    shared: &Shared,
    item: Item,
    cx: &mut App,
) -> Option<AnyElement> {
    let Item { ix, tail } = item;
    let drawn = render_row_inner(model, session, shared, item, cx)?;
    let m = model.borrow();
    if m.alone.is_none() {
        return Some(column(drawn).into_any_element());
    }
    // The subagent tab starts with what it was asked and ends with its report.
    let head = (ix == 0).then(|| alone::head(&m, &shared.texts, cx)).flatten();
    let foot = (ix == tail).then(|| alone::foot(&m, &shared.texts, cx)).flatten();
    Some(column(v_flex().w_full().children(head).child(drawn).children(foot)).into_any_element())
}

/// What a row shows, copied out so the log isn't held while it's drawn.
#[expect(clippy::large_enum_variant, reason = "made for one row and matched straight away")]
enum Shown {
    /// With how the log is read, which says whether the subagents' work is kept apart, in the subagent tab.
    Tail(Tail, By),
    Event(SessionEvent, Around),
    Pending(PendingPrompt, Vec<Carried>),
    Unloaded,
    /// Drawn elsewhere, so its own row is empty: a subagent's event, which its call draws, or in the subagent tab,
    /// its last words, which its report says.
    InCall,
}

fn render_row_inner(
    model: &Rc<RefCell<Model>>,
    session: &OpenSession,
    shared: &Shared,
    item: Item,
    cx: &mut App,
) -> Option<AnyElement> {
    let Item { ix, tail } = item;
    let find = model.borrow().find.clone();
    mark_found(model, ix, &find, &shared.texts, cx);
    let shown = {
        let m = model.borrow();
        session.read(|log| {
            if ix >= tail || ix >= log.row_count() {
                return Shown::Tail(log.tail().clone(), log.by());
            }
            match log.row(ix) {
                Row::Event(event) if drawn_in_its_call(log, &m, event) || alone::repeats_report(log, &m, ix) => {
                    Shown::InCall
                }
                Row::Event(event) => Shown::Event(event.clone(), Around::read(log, &m, event.seq, &event.body)),
                Row::Pending(prompt) => Shown::Pending(prompt.clone(), attachments::carried(&m, &prompt.attachments)),
                Row::Unloaded => Shown::Unloaded,
            }
        })
    };
    Some(match shown {
        Shown::Tail(tail, by) => {
            let m = model.borrow();
            render_tail(&tail, &m.at_work, shared, by, m.folds.thinking_open(), cx)
        }
        Shown::Event(event, around) => {
            attachments::fetch_missing(model, &shared.this, Some(event.seq), around.images(), cx);
            render_event(&event, &around, shared, &find, cx)
        }
        Shown::Pending(prompt, carried) => {
            let key = ItemKey::Prompt(prompt.client_msg_id);
            let text = Prompt { text: &prompt.text, find: &find };
            prompt_bubble(&key, text, &carried, Progress::Sending, shared, cx)
        }
        Shown::InCall => empty(),
        Shown::Unloaded => return None,
    })
}

/// Has the views of the Markdown with matches in the row at `ix`, as counting found it, draw them. Each is kept from
/// the end of the row's drawing, rather than every row's when they're counted.
fn mark_found(model: &Rc<RefCell<Model>>, ix: usize, find: &FindLook, texts: &LongTexts, cx: &mut App) {
    if find.needle().is_none() {
        return;
    }
    let Some(found) = model.borrow().found_markdown.get(&ix).cloned() else { return };
    for id in found.iter() {
        texts.mark(id, find, cx);
    }
}

/// Text the agent wrote, under the same ID while it streams and once it's recorded.
fn agent_text(texts: &LongTexts, key: &ItemKey, text: &str) -> TextView {
    texts.markdown(text_id(key), text)
}

/// What the text the agent wrote under `key` is drawn by.
fn text_id(key: &ItemKey) -> ElementId {
    key.element("text")
}

fn row() -> Div {
    v_flex().w_full().pb_4().gap_2()
}

fn empty() -> AnyElement {
    div().into_any_element()
}

fn muted_line(text: impl Into<SharedString>) -> AnyElement {
    row().child(div().text_xs().text_color(look::faint()).child(text.into())).into_any_element()
}

/// What the app says when the agent stopped, and why, with what it last printed, if it stopped unexpectedly.
fn agent_exited(seq: Seq, reason: ExitReason, stderr_tail: &[String], find: &FindLook, cx: &App) -> AnyElement {
    match reason {
        ExitReason::Requested => empty(),
        ExitReason::HubRestart => {
            muted_line("The app restarted while the agent was working. Send a message to continue.")
        }
        ExitReason::WorkerLost | ExitReason::Unexpected | ExitReason::StartFailed | ExitReason::Unknown => {
            const STOPPED: &str = "The agent stopped unexpectedly. ";
            let printed = exit_output(reason, stderr_tail).unwrap_or_default();
            let marks = find.marks(&(ItemKey::Event(seq, 0), Part::Exit), printed).moved(STOPPED.len());
            marked_line(format!("{STOPPED}{printed}"), &marks, cx.theme().danger)
        }
    }
}

/// What the agent last printed, which the app says when it stopped unexpectedly.
pub(crate) fn exit_output(reason: ExitReason, stderr_tail: &[String]) -> Option<&str> {
    match reason {
        ExitReason::Requested | ExitReason::HubRestart => None,
        ExitReason::WorkerLost | ExitReason::Unexpected | ExitReason::StartFailed | ExitReason::Unknown => {
            stderr_tail.last().map(String::as_str)
        }
    }
}

/// Something the hub or the agent's CLI wanted said, in the colour of how much it matters, with its matches.
fn notice(seq: Seq, level: NoticeLevel, message: &str, find: &FindLook, cx: &App) -> AnyElement {
    let color = match level {
        NoticeLevel::Error => cx.theme().danger,
        NoticeLevel::Warning => cx.theme().warning,
        NoticeLevel::Info | NoticeLevel::Unknown => cx.theme().muted_foreground,
    };
    let marks = find.marks(&(ItemKey::Event(seq, 0), Part::Notice), message);
    marked_line(message.to_string(), &marks, color)
}

fn render_event(event: &SessionEvent, around: &Around, shared: &Shared, find: &FindLook, cx: &App) -> AnyElement {
    let (seq, body) = (event.seq, &event.body);
    match body {
        // Asked in the composer's place while it waits, so it's drawn here once it's no longer waiting, as a call.
        EventBody::ApprovalRequested { tool_name, input, .. } if tool_name == questions::TOOL_NAME => {
            match &around.asked {
                Some(asked) => asked::draw(seq, input, asked, shared, cx),
                None => empty(),
            }
        }
        // The compaction says all there is of what asked for it, and of a turn that was only it.
        EventBody::CompactionAsked { .. } | EventBody::TurnEnded { only_compacted: true, .. } => empty(),
        EventBody::AgentCompacted { .. } => compaction::of(seq, body, around, shared, cx),
        EventBody::UserPrompt { text, client_msg_id, .. } => {
            let (key, progress) =
                (ItemKey::prompt(seq, client_msg_id.as_ref()), Progress::Recorded(seq, around.prompt));
            let text = Prompt { text, find };
            prompt_bubble(&key, text, &around.carried, progress, shared, cx)
        }
        EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => {
            let rendered: Vec<AnyElement> = blocks
                .iter()
                .enumerate()
                .zip(&around.blocks)
                .filter_map(|((_, block), drawing)| render_block(seq, block, drawing, shared, find, cx))
                .collect();
            if rendered.is_empty() {
                return empty();
            }
            // Calls and thinking in a row read as one list, so they sit closer together than paragraphs do.
            let only_lines = blocks.iter().all(is_a_line);
            row().when(only_lines, |d| d.pb_2().gap_2()).children(rendered).into_any_element()
        }
        EventBody::TurnEnded { .. } => turn_end::line(event, around.agent, find, cx),
        EventBody::ApprovalRequested { approval_id, tool_name, input, description, .. } => {
            approval_card(seq, approval_id, tool_name, input, description.as_deref(), around.approval_open, shared, cx)
        }
        // A question's line says what became of it.
        EventBody::ApprovalResolved { .. } if around.closes_a_question => empty(),
        EventBody::ApprovalResolved { outcome: ApprovalOutcome::Cancelled, .. } => muted_line("Approval cancelled"),
        EventBody::ApprovalResolved { outcome: ApprovalOutcome::Denied, .. } => muted_line("Denied"),
        EventBody::AgentMode { permission_mode } => muted_line(mode::mode_change(around.mode_before, *permission_mode)),
        EventBody::AgentExited { reason, stderr_tail, .. } => agent_exited(seq, *reason, stderr_tail, find, cx),
        EventBody::Notice { level, message } => notice(seq, *level, message, find, cx),
        // These tell the hub things, and there's nothing here for a person.
        EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::AgentLimits { .. }
        | EventBody::SubagentStarted { .. }
        | EventBody::SubagentEnded { .. }
        | EventBody::AgentOther { .. }
        | EventBody::AgentCompacting { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::CacheExpired { .. }
        | EventBody::Unknown => empty(),
    }
}

/// A call, its result or thinking, which read as lines of one list.
fn is_a_line(block: &ContentBlock) -> bool {
    matches!(block, ContentBlock::ToolUse { .. } | ContentBlock::ToolResult { .. } | ContentBlock::Thinking { .. })
}

/// A line of `text` in `color`, where nothing's found.
fn colored_line(text: String, color: Hsla) -> AnyElement {
    row().child(div().text_sm().text_color(color).child(text)).into_any_element()
}

/// A line of `text` in `color`, with its matches.
fn marked_line(text: String, marks: &Marks, color: Hsla) -> AnyElement {
    row().child(div().text_sm().text_color(color).child(find::marked(text.into(), marks, &[], None))).into_any_element()
}

fn render_block(
    seq: Seq,
    block: &ContentBlock,
    drawing: &BlockAround,
    shared: &Shared,
    find: &FindLook,
    cx: &App,
) -> Option<AnyElement> {
    let key = &drawing.key;
    let (fold, open, whole) = (drawing.fold, drawing.open, drawing.whole);
    let toggle = Toggle { seq, key, fold, open, whole, shared, find };
    Some(match block {
        ContentBlock::Text { text, images } => {
            let view = agent_text(&shared.texts, key, text).image_source(shared.linked.resolver(seq, images));
            find::markdown_view(view, &(key.clone(), Part::Text), find, None)
        }
        ContentBlock::Thinking { text } if text.is_empty() => return None,
        ContentBlock::Thinking { text } => thinking::draw(text, &toggle, cx),
        // These are drawn from their approvals, which have everything that was asked.
        ContentBlock::ToolUse { name, .. } if calls::drawn_from_its_approval(name) => return None,
        ContentBlock::ToolUse { name, input, .. } => {
            tools::call(&toggle, name, input, &drawing.reply, drawing.subagent.as_ref(), cx)
        }
        ContentBlock::ToolResult { .. } if drawing.with_call => return None,
        ContentBlock::ToolResult { .. } => tools::result(&toggle, drawing.reply.result()?, cx),
        ContentBlock::Image { .. } => {
            div().text_xs().text_color(cx.theme().muted_foreground).child("Image").into_any_element()
        }
        ContentBlock::Unknown => return None,
    })
}
