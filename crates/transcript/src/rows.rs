//! What each row of the log looks like.

use std::cell::RefCell;
use std::rc::Rc;

use client_core::OpenSession;
use client_core::session_log::{By, Row};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, Hsla, InteractiveElement as _, IntoElement, ParentElement, SharedString, Stateful,
    Styled, WeakEntity, div, px,
};
use look::LineText;
use protocol::event::{ApprovalOutcome, ContentBlock, EventBody, ExitReason, NoticeLevel, SessionEvent};
use protocol::model::Seq;
use protocol::tail::Tail;

use crate::attachments;
use crate::find::{self, FindLook, Marks, Part};
use crate::item_key::ItemKey;
use crate::linked_images::LinkedImages;
use crate::long_texts::LongTexts;
use crate::prose::{Prose, TextAt};
use crate::selection::Selection;
use crate::transcript::{Model, Transcript};
use crate::{calls, questions};

mod alone;
mod approval;
mod around;
mod asked;
mod cache_rule;
mod column;
pub(crate) mod compaction;
mod line;
mod mode;
mod prompt;
mod shown;
mod tail;
mod text;
mod thinking;
mod tools;
mod turn_end;
mod turn_times;

pub(crate) use alone::rows_repeating_report;
pub use alone::subagent_standing;
pub(crate) use around::{NEAR, REACH};
pub(crate) use column::room_under;
pub use column::{ABOVE_THE_COMPOSER, around_column, in_column};
pub(crate) use shown::{Found, of_row as found_in_row, rows_drawing, rows_to_count};

use approval::approval_card;
pub(crate) use around::SubagentCalls;
use around::{Around, BlockAround, drawn_in_its_call, drawn_in_row};
use column::column;
use prompt::{Progress, Prompt, prompt_bubble};
use tail::{EndOfChat, Waiting, render_tail};
use text::{agent_text, recorded};
use tools::Toggle;

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
    /// The texts the rows draw from the kit's blocks, and what's selected of them.
    pub(crate) prose: Prose,
    pub(crate) selection: Selection,
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
    /// With how the log is read, which says whether the subagents' work is kept apart, in the subagent tab, and the
    /// prompts the agent hasn't read yet.
    Tail(Tail, By, Vec<Waiting>),
    /// The place in the log of the row's own event, which its texts are placed by, and the event it draws, which for a
    /// prompt is the one the row's event settled.
    Event(Seq, SessionEvent, Around),
    Unloaded,
    /// Drawn elsewhere, so its own row is empty: a subagent's event, which its call draws, in the subagent tab, its
    /// last words, which its report says, or a prompt, drawn where the agent took it or at the end.
    InCall(Seq),
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
                return Shown::Tail(log.tail().clone(), log.by(), tail::waiting(log, &m));
            }
            match log.row(ix) {
                Row::Event(event) if drawn_in_its_call(log, &m, event) || alone::repeats_report(log, &m, ix) => {
                    Shown::InCall(event.seq)
                }
                Row::Event(event) => match drawn_in_row(log, event) {
                    Some(drawn) => {
                        Shown::Event(event.seq, drawn.clone(), Around::read(log, &m, drawn.seq, &drawn.body))
                    }
                    None => Shown::InCall(event.seq),
                },
                Row::Unloaded => Shown::Unloaded,
            }
        })
    };
    Some(match shown {
        Shown::Tail(tail, by, waiting) => {
            for prompt in &waiting {
                if let Waiting::Recorded(event, around) = prompt {
                    attachments::fetch_missing(model, &shared.this, Some(event.seq), around.images(), cx);
                }
            }
            let m = model.borrow();
            let end = EndOfChat { tail: &tail, waiting: &waiting, at_work: &m.at_work };
            render_tail(&end, shared, by, m.folds.thinking_open(), &find, cx)
        }
        // Each text an event's row no longer draws is hidden, such as one in a fold that's been closed, or all of
        // them once it's drawn elsewhere.
        Shown::Event(row, event, around) => {
            attachments::fetch_missing(model, &shared.this, Some(event.seq), around.images(), cx);
            shared.prose.drawing_event(row, || render_event(&event, &around, Some(row), shared, &find, cx))
        }
        Shown::InCall(seq) => shared.prose.drawing_event(seq, empty),
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

fn row() -> Div {
    v_flex().w_full().pb_4().gap_2()
}

fn empty() -> AnyElement {
    div().into_any_element()
}

/// A rule across the chat with `middle` in its middle, as a compaction and a cache that had expired are drawn.
fn rule_across(middle: impl IntoElement, cx: &App) -> Div {
    let border = cx.theme().border;
    let rule = || div().flex_1().h(px(1.)).bg(border);
    h_flex().w_full().items_center().gap_3().child(rule()).child(middle).child(rule())
}

/// What's said in the middle of a rule across the chat, small and faint.
fn rule_middle(id: impl Into<ElementId>) -> Stateful<Div> {
    h_flex().id(id).flex_none().gap_1p5().px_2().py_0p5().text_xs().text_color(look::faint())
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

/// `event`, drawn in the row of the event at `place` in the log, which is its own but for a prompt drawn where the
/// agent took it, or at the end of the chat, in no event's row, for `None`.
fn render_event(
    event: &SessionEvent,
    around: &Around,
    place: Option<Seq>,
    shared: &Shared,
    find: &FindLook,
    cx: &App,
) -> AnyElement {
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
        EventBody::CacheExpired { idle_ms, read_tokens, written_tokens } => {
            cache_rule::line(seq, *idle_ms, *read_tokens, *written_tokens, cx)
        }
        EventBody::UserPrompt { text, client_msg_id, .. } => {
            let (key, progress) =
                (ItemKey::prompt(seq, client_msg_id.as_ref()), Progress::recorded(place, around.prompt));
            let text = Prompt { text, find };
            prompt_bubble(&key, text, &around.carried, progress, shared, cx)
        }
        EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => {
            let rendered: Vec<AnyElement> = blocks
                .iter()
                .enumerate()
                .zip(&around.blocks)
                .filter_map(|((ix, block), drawing)| {
                    render_block(seq, InRow::Own(ix), block, drawing, shared, find, cx)
                })
                .collect();
            if rendered.is_empty() {
                return empty();
            }
            // Calls and thinking in a row read as one list, so they sit closer together than paragraphs do.
            let only_lines = blocks.iter().all(is_a_line);
            row().when(only_lines, |d| d.pb_2().gap_2()).children(rendered).into_any_element()
        }
        EventBody::TurnEnded { .. } => turn_end::line(event, around.agent, around.cache.as_ref(), find, cx),
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

/// Which row a block is drawn in: its event's own, as the `n`th of the event's blocks, or a call's, as a subagent's
/// words are.
#[derive(Clone, Copy)]
pub(in crate::rows) enum InRow {
    Own(usize),
    Call,
}

/// A block of the event at `seq`, drawn in the row `in_row` says.
fn render_block(
    seq: Seq,
    in_row: InRow,
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
        // In a row of its own, the transcript draws it from the kit's blocks, and selects it itself.
        ContentBlock::Text { text, images } if let InRow::Own(block) = in_row => {
            recorded(TextAt { seq, block }, &(key.clone(), Part::Text), text, Some(images), find, shared)
        }
        // In a call's row, as a subagent's words are, it's the kit's text view.
        ContentBlock::Text { text, images } => {
            let view = agent_text(&shared.texts, key, text).image_source(shared.linked.resolver(seq, images));
            find::markdown_view(view, &(key.clone(), Part::Text), find, None)
        }
        ContentBlock::Thinking { text } if text.is_empty() => return None,
        ContentBlock::Thinking { text } => thinking::draw(text, &toggle, in_row, cx),
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
