use super::{
    alone, around, asked, cache_rule, column, compaction, failed_request, message, mode, notices, own_turn, plan_card,
    prompt, tail, turn_end, update_rule,
};
use crate::find::{FindLook, Marks};
use crate::item_key::ItemKey;
use crate::linked_images::LinkedImages;
use crate::selection::Selection;
use crate::texts::{Of, TextAt, Texts};
use crate::transcript::{Model, Transcript};
use crate::{attachments, find};
use around::{Around, Closes, asks_questions, denied, drawn_in_its_call, drawn_in_row};
use client_core::session_log::{By, Row, SessionLog};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, ElementId, Hsla, InteractiveElement as _, IntoElement, ParentElement, SharedString, Stateful,
    Styled, WeakEntity, div,
};
use look::{LineText, Typeset as _};
use notices::{agent_exited, notice};
use prompt::{Prompt, prompt_bubble};
use protocol::capability::turns::OwnTurn;
use protocol::event::{EventBody, SessionEvent};
use protocol::model::Seq;
use protocol::tail::Tail;
use std::cell::RefCell;
use std::rc::Rc;
use tail::{EndOfChat, Waiting, render_tail};

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

/// Draws the list's row `item` from `log`, the version of the log whose rows the list was last told of, which every
/// row of a frame is drawn from. So a frame draws each of the log's events in one place, however much the log takes
/// while it's drawn: a prompt where it waits or where the agent took it, and a reply at the end of the chat as it's
/// written or from its record. The list's last row is the end of the chat.
pub(crate) fn render_row(
    model: &Rc<RefCell<Model>>,
    log: &SessionLog,
    shared: &Shared,
    item: Item,
    cx: &mut App,
) -> Option<AnyElement> {
    let Item { ix, tail } = item;
    let drawn = render_row_inner(model, log, shared, item, cx)?;
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
        Of::Event(_) | Of::Document(_) => None,
    }
}

/// What a row shows, from the version of the log the frame is drawn from.
#[expect(clippy::large_enum_variant, reason = "made for a row, and drawn at once")]
enum Shown<'a> {
    /// With how the log is read, which says whether the subagents' work is kept apart, in the subagent tab, and the
    /// prompts the agent hasn't read yet.
    Tail(&'a Tail, By, Vec<Waiting>),
    /// The place in the log of the row's own event, which its texts are placed by, and the event it draws, which for a
    /// prompt is the one the row's event settled.
    Event(Seq, &'a SessionEvent, Around),
    Unloaded,
    /// Drawn elsewhere, so its own row is empty: a subagent's event, which its call draws, in the subagent tab, its
    /// last words, which its report says, or a prompt, drawn where the agent took it or at the end.
    InCall(Seq),
}

/// Draws the list's row `item`, as [`render_row`] says.
fn render_row_inner(
    model: &Rc<RefCell<Model>>,
    log: &SessionLog,
    shared: &Shared,
    item: Item,
    cx: &mut App,
) -> Option<AnyElement> {
    let Item { ix, tail } = item;
    let find = model.borrow().find.clone();
    let shown = {
        let m = model.borrow();
        if ix >= tail { Shown::Tail(log.tail(), log.by(), tail::waiting(log, &m)) } else { Shown::of(log, &m, ix) }
    };
    render_shown(model, shared, &find, shown, cx)
}

impl<'a> Shown<'a> {
    /// What the log's row `row` shows: its event, with what's around it, or nothing, for an event drawn elsewhere.
    fn of(log: &'a SessionLog, m: &Model, row: usize) -> Self {
        match log.row(row) {
            Row::Event(event) if drawn_in_its_call(log, m, event) || alone::repeats_report(log, m, row) => {
                Shown::InCall(event.seq)
            }
            Row::Event(event) => match drawn_in_row(log, event) {
                Some(drawn) => Shown::Event(event.seq, drawn, Around::read(log, m, drawn.seq, &drawn.body)),
                None => Shown::InCall(event.seq),
            },
            Row::Unloaded => Shown::Unloaded,
        }
    }
}

/// Draws what `shown` says, if it's loaded.
fn render_shown(
    model: &Rc<RefCell<Model>>,
    shared: &Shared,
    find: &FindLook,
    shown: Shown,
    cx: &mut App,
) -> Option<AnyElement> {
    Some(match shown {
        Shown::Tail(tail, by, waiting) => {
            for prompt in &waiting {
                if let Waiting::Recorded(event, around) = prompt {
                    attachments::fetch_missing(model, &shared.this, Some(event.seq), around.images(), cx);
                }
            }
            let m = model.borrow();
            let (at_work, folds, alone) = (&m.at_work, &m.folds, m.alone.is_some());
            let end = EndOfChat { tail, waiting: &waiting, at_work, folds, alone };
            shared.texts.drawing(Of::Tail, || render_tail(&end, shared, by, cx))
        }
        // Each text an event's row no longer draws is hidden, such as one in a fold that's been closed, or all of
        // them once it's drawn elsewhere.
        Shown::Event(row, event, around) => {
            attachments::fetch_missing(model, &shared.this, Some(event.seq), around.images(), cx);
            let prompt_at = TextAt::only(row, 0);
            shared.texts.drawing(Of::Event(row), || render_event(event, &around, prompt_at, shared, find, cx))
        }
        Shown::InCall(seq) => shared.texts.drawing(Of::Event(seq), empty),
        Shown::Unloaded => return None,
    })
}

pub(super) fn row() -> Div {
    v_flex().w_full().pb(look::Space::Large).gap(look::Space::Small)
}

pub(super) fn empty() -> AnyElement {
    div().into_any_element()
}

/// How big an icon in the middle of a rule across the chat is, among its detail's text.
pub(crate) const RULE_ICON: look::IconSize = look::IconSize::Small;

/// The row of a rule across the chat, with `middle` in its middle and nothing under it.
pub(super) fn rule_row(middle: impl IntoElement, cx: &App) -> AnyElement {
    row().pt(look::Space::Small).child(rule_across(middle, cx)).into_any_element()
}

/// A rule across the chat with `middle` in its middle, as a compaction, a cache that had expired and an update are
/// drawn.
pub(super) fn rule_across(middle: impl IntoElement, cx: &App) -> Div {
    let border = look::colors(cx).edge;
    let rule = || div().flex_1().h(look::EDGE).bg(border);
    h_flex().w_full().items_center().gap(look::Space::Medium).child(rule()).child(middle).child(rule())
}

/// What's said in the middle of a rule across the chat, small and faint.
pub(super) fn rule_middle(id: impl Into<ElementId>, cx: &App) -> Stateful<Div> {
    h_flex()
        .id(id)
        .flex_none()
        .gap(look::Space::Small)
        .px(look::Space::Small)
        .py(look::Space::XSmall)
        .typeset(look::Type::Detail)
        .text_color(look::colors(cx).text_faint)
}

pub(super) fn muted_line(text: impl Into<SharedString>, cx: &App) -> AnyElement {
    row()
        .child(div().typeset(look::Type::Detail).text_color(look::colors(cx).text_faint).child(text.into()))
        .into_any_element()
}

/// `event`, drawn in the row of an event, which is its own but for a prompt drawn where the agent took it, or at the
/// end of the chat. A prompt's text goes at `prompt_at`, and the texts of any other event by its place in the log.
pub(super) fn render_event(
    event: &SessionEvent,
    around: &Around,
    prompt_at: TextAt,
    shared: &Shared,
    find: &FindLook,
    cx: &App,
) -> AnyElement {
    let (seq, body) = (event.seq, &event.body);
    match body {
        // Said in a later failure's card, which hosts the failures in a row.
        _ if around.failures.in_a_later_card() => empty(),
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
        EventBody::RequestFailed { failure, .. } => failed_request::card(seq, failure, around, shared, find, cx),
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
            message::draw(seq, blocks, around, shared, find, cx)
        }
        EventBody::TurnEnded { .. } => turn_end::line(event, around.agent.clone(), around.cache.as_ref(), find, cx),
        // A plan's card. A tool's use is asked in the composer's place too, and its call's own line is what the
        // transcript keeps of it, with what became of it after.
        EventBody::ApprovalRequested { .. } => plan_card(seq, body, around.approval_open, shared),
        // A question's line says what became of it, and so does a denied call's, as `Closes` says.
        EventBody::ApprovalResolved { .. } if matches!(around.closes, Closes::SaidElsewhere) => empty(),
        EventBody::ApprovalResolved { decision: None, .. } => muted_line("Approval cancelled", cx),
        EventBody::ApprovalResolved { .. } if denied(body) => match &around.closes {
            Closes::ForASubagent { tool, subagent } => muted_line(format!("Denied {tool} for {subagent}"), cx),
            Closes::Unsaid | Closes::SaidElsewhere => muted_line("Denied", cx),
        },
        EventBody::AgentMode { mode } => muted_line(mode::mode_change(around.mode_before, *mode), cx),
        EventBody::AgentExited { reason, stderr_tail, .. } => agent_exited(seq, *reason, stderr_tail, find, cx),
        EventBody::Notice { level, message, .. } => notice(seq, *level, message, shared, find, cx),
        EventBody::OwnTurn(OwnTurn::TaskEnded { ended }) => own_turn::lines(event, ended, shared, cx),
        // These tell the hub things, and there's nothing here for a person. An agent's start says something only when
        // it's on another build than the chat's agent last was, above.
        EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::OwnTurn(_)
        | EventBody::AgentLimits { .. }
        | EventBody::SubagentStarted { .. }
        | EventBody::SubagentContext { .. }
        | EventBody::SubagentEnded { .. }
        | EventBody::TaskChanged { .. }
        | EventBody::AgentOther { .. }
        | EventBody::AgentCompacting { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::Unknown => empty(),
    }
}

/// A line of `text` in `color`, where nothing's found.
pub(super) fn colored_line(text: String, color: Hsla) -> AnyElement {
    row().child(div().typeset(look::Type::Body).text_color(color).child(text)).into_any_element()
}

/// A line of `text` in `color`, with its matches.
pub(super) fn marked_line(text: String, marks: &Marks, color: Hsla) -> AnyElement {
    row()
        .child(div().typeset(look::Type::Body).text_color(color).child(find::marked(text.into(), marks, &[], None)))
        .into_any_element()
}
