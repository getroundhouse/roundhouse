//! What each row of the log looks like.

use std::cell::RefCell;
use std::rc::Rc;

use client_core::OpenSession;
use client_core::calls::Recorded;
use client_core::session_log::{By, Row};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, Hsla, InteractiveElement as _, IntoElement, ParentElement, SharedString, Stateful,
    Styled, WeakEntity, div, px,
};
use look::LineText;
use protocol::event::{ApprovalOutcome, ContentBlock, EventBody, SessionEvent};
use protocol::model::Seq;
use protocol::tail::Tail;

use crate::attachments;
use crate::find::{self, FindLook, Marks, Part};
use crate::item_key::ItemKey;
use crate::linked_images::LinkedImages;
use crate::selection::Selection;
use crate::texts::{Of, TextAt, Texts};
use crate::transcript::{Model, Transcript};

mod alone;
mod around;
mod asked;
mod cache_rule;
mod card;
mod column;
pub(crate) mod compaction;
mod failed_request;
mod line;
mod mode;
mod notices;
mod plan_card;
mod prompt;
mod shown;
mod tail;
mod text;
mod thinking;
mod tools;
mod turn_end;
mod turn_times;
mod update_rule;

pub(crate) use alone::rows_repeating_report;
pub use alone::subagent_standing;
pub(crate) use around::{NEAR, REACH, retry_at};
pub(crate) use column::room_under;
pub use column::{ABOVE_THE_COMPOSER, around_column, in_column};
pub(crate) use shown::{Found, of_row as found_in_row, rows_drawing, rows_to_count};
pub(crate) use tools::ChangeAsked;

pub(crate) use around::SubagentCalls;
use around::{Around, BlockAround, Closes, asks_questions, drawn_in_its_call, drawn_in_row};
use column::column;
use notices::{agent_exited, exit_output, notice};
use plan_card::plan_card;
use prompt::{Prompt, prompt_bubble};
use tail::{EndOfChat, Waiting, render_tail};
use text::{from_blocks, written};
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
    /// The texts the rows draw from the kit's blocks, and what's selected of them.
    pub(crate) texts: Texts,
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
    let here = |of| list_row_of(of, tail) == Some(ix);
    let head = here(Of::Head).then(|| shared.texts.drawing(Of::Head, || alone::head(&m, shared, cx))).flatten();
    let foot = here(Of::Foot).then(|| shared.texts.drawing(Of::Foot, || alone::foot(&m, shared, cx))).flatten();
    Some(column(v_flex().w_full().children(head).child(drawn).children(foot)).into_any_element())
}

/// The list's row that draws what `of` says, but for an event's, which is where the log has it, when the list's tail,
/// its last row, is `tail`. The tail is the end of the chat, and in the subagent tab, its first row draws the tab's
/// head above what else it draws, and the tail its foot below.
pub(crate) fn list_row_of(of: Of, tail: usize) -> Option<usize> {
    match of {
        Of::Head => Some(0),
        Of::Tail | Of::Foot => Some(tail),
        Of::Event(_) => None,
    }
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
            let pull_request = m.pull_request.as_ref();
            let end = EndOfChat { tail: &tail, waiting: &waiting, at_work: &m.at_work, folds: &m.folds, pull_request };
            shared.texts.drawing(Of::Tail, || render_tail(&end, shared, by, cx))
        }
        // Each text an event's row no longer draws is hidden, such as one in a fold that's been closed, or all of
        // them once it's drawn elsewhere.
        Shown::Event(row, event, around) => {
            attachments::fetch_missing(model, &shared.this, Some(event.seq), around.images(), cx);
            let prompt_at = TextAt::only(row, 0);
            shared.texts.drawing(Of::Event(row), || render_event(&event, &around, prompt_at, shared, &find, cx))
        }
        Shown::InCall(seq) => shared.texts.drawing(Of::Event(seq), empty),
        Shown::Unloaded => return None,
    })
}

fn row() -> Div {
    v_flex().w_full().pb_4().gap_2()
}

fn empty() -> AnyElement {
    div().into_any_element()
}

/// How big an icon in the middle of a rule across the chat is.
pub(crate) const RULE_ICON: f32 = 13.0;

/// The row of a rule across the chat, with `middle` in its middle and nothing under it.
fn rule_row(middle: impl IntoElement, cx: &App) -> AnyElement {
    row().pt_2().child(rule_across(middle, cx)).into_any_element()
}

/// A rule across the chat with `middle` in its middle, as a compaction, a cache that had expired and an update are
/// drawn.
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

/// A message of the agent's, or what came back to it, recorded at `seq`: each of its `blocks` as `around` says.
fn message(
    seq: Seq,
    blocks: &[ContentBlock],
    around: &Around,
    shared: &Shared,
    find: &FindLook,
    cx: &App,
) -> AnyElement {
    let rendered: Vec<AnyElement> = blocks
        .iter()
        .enumerate()
        .zip(&around.blocks)
        .filter_map(|((ix, block), drawing)| render_block(seq, InRow::Own(ix), block, drawing, shared, find, cx))
        .collect();
    if rendered.is_empty() {
        return empty();
    }
    // Calls and thinking in a row read as one list, so they sit closer together than paragraphs do.
    let only_lines = blocks.iter().all(is_a_line);
    row().when(only_lines, |d| d.pb_2().gap_2()).children(rendered).into_any_element()
}

/// `event`, drawn in the row of an event, which is its own but for a prompt drawn where the agent took it, or at the
/// end of the chat. A prompt's text goes at `prompt_at`, and the texts of any other event by its place in the log.
fn render_event(
    event: &SessionEvent,
    around: &Around,
    prompt_at: TextAt,
    shared: &Shared,
    find: &FindLook,
    cx: &App,
) -> AnyElement {
    let (seq, body) = (event.seq, &event.body);
    match body {
        // Asked in the composer's place while it waits, so it's drawn here once it's no longer waiting, as a call.
        EventBody::ApprovalRequested { .. } if asks_questions(body) => match &around.asked {
            Some(asked) => asked::draw(seq, asked, shared, cx),
            None => empty(),
        },
        // The compaction says all there is of what asked for it, and of a turn that was only it, and a failed
        // request's card all there is of the turn that failed on it.
        EventBody::CompactionAsked { .. }
        | EventBody::TurnEnded { only_compacted: true, .. }
        | EventBody::TurnEnded { request_failed: true, .. } => empty(),
        EventBody::RequestFailed { failure, .. } => failed_request::card(seq, failure, around.retry, shared, find, cx),
        EventBody::AgentCompacted { .. } => compaction::of(seq, body, around, shared, cx),
        EventBody::CacheExpired { idle_ms, read_tokens, written_tokens } => {
            cache_rule::line(seq, *idle_ms, *read_tokens, *written_tokens, cx)
        }
        EventBody::AgentStarted { build: Some(build), previous_build: Some(from), .. } => {
            update_rule::line(seq, from, build, cx)
        }
        EventBody::UserPrompt { client_msg_id, prompt, .. } => {
            let key = ItemKey::prompt(seq, client_msg_id.as_ref());
            let text = Prompt { text: &prompt.text, find, at: prompt_at };
            prompt_bubble(&key, text, &around.carried, Some(around.prompt), shared, cx)
        }
        EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } => {
            message(seq, blocks, around, shared, find, cx)
        }
        EventBody::TurnEnded { .. } => turn_end::line(event, around.agent.clone(), around.cache.as_ref(), find, cx),
        // A plan's card. A tool's use is asked in the composer's place too, and its call's own line is what the
        // transcript keeps of it, with what became of it after.
        EventBody::ApprovalRequested { .. } => plan_card(seq, body, around.approval_open, shared, cx),
        // A question's line says what became of it, and so does a denied call's, as `Closes` says.
        EventBody::ApprovalResolved { .. } if matches!(around.closes, Closes::SaidElsewhere) => empty(),
        EventBody::ApprovalResolved { outcome: ApprovalOutcome::Cancelled, .. } => muted_line("Approval cancelled"),
        EventBody::ApprovalResolved { outcome: ApprovalOutcome::Denied, .. } => match &around.closes {
            Closes::ForASubagent { tool, subagent } => muted_line(format!("Denied {tool} for {subagent}")),
            Closes::Unsaid | Closes::SaidElsewhere => muted_line("Denied"),
        },
        EventBody::AgentMode { permission_mode } => muted_line(mode::mode_change(around.mode_before, *permission_mode)),
        EventBody::AgentExited { reason, stderr_tail, .. } => agent_exited(seq, *reason, stderr_tail, find, cx),
        EventBody::Notice { level, message, .. } => notice(seq, *level, message, shared, find, cx),
        // These tell the hub things, and there's nothing here for a person. An agent's start says something only when
        // it's on another build than the chat's agent last was, above.
        EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::AgentLimits { .. }
        | EventBody::SubagentStarted { .. }
        | EventBody::SubagentContext { .. }
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
/// words are, nested in the `n`th block of that row's event. Either way, `n` is the block its texts are placed in. The
/// questions the agent asked are each drawn as a block of their own, in their order, so each one's texts are together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::rows) enum InRow {
    Own(usize),
    Nested(usize),
}

impl InRow {
    /// The row what this block nests is drawn in: the same row, nested in the same block of its event, however deep.
    pub(in crate::rows) fn nesting(self) -> Self {
        match self {
            InRow::Own(block) | InRow::Nested(block) => InRow::Nested(block),
        }
    }
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
    let (fold, open) = (drawing.fold, drawing.open);
    let toggle = Toggle { seq, key, fold, open, shared, find, in_row };
    Some(match block {
        ContentBlock::Text { text, images } => written(&toggle, Part::Text, text, None, Some(images)),
        ContentBlock::Thinking { text } if text.is_empty() => return None,
        ContentBlock::Thinking { text } => thinking::draw(text, &toggle, cx),
        ContentBlock::ToolUse { .. } => {
            // One that asks the person something is drawn from its approval, which has everything that was asked.
            let recorded = Recorded::of(block).filter(|call| !call.asks_the_person())?;
            tools::call(&toggle, recorded, &drawing.reply, drawing.subagent.as_ref(), cx)
        }
        ContentBlock::ToolResult { .. } if drawing.with_call => return None,
        ContentBlock::ToolResult { .. } => tools::result(&toggle, drawing.reply.result()?, cx),
        ContentBlock::Image { .. } => {
            div().text_xs().text_color(cx.theme().muted_foreground).child("Image").into_any_element()
        }
        ContentBlock::Unknown => return None,
    })
}
