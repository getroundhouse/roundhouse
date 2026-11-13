use super::super::around::Around;
use super::super::line::{HEIGHT, still_icon};
use super::super::prompt::{Prompt, prompt_bubble};
use super::super::{Shared, empty, from_blocks, render_event, row, thinking, turn_times};
use super::background;
use crate::attachments::Carried;
use crate::find::{FindLook, Part};
use crate::folds::Folds;
use crate::item_key::ItemKey;
use crate::texts::{TextAt, Written};
use crate::transcript::{AtWork, Model, PullRequestStanding};
use crate::{attachments, find};
use client_core::session_log::{By, SessionLog};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement, Styled, TestSupportExt as _, div, px,
};
use look::{LineText, Signal};
use protocol::event::SessionEvent;
use protocol::model::QueuedPrompt;
use protocol::tail::{BlockKind, Compacting, Tail};
use std::time::Duration;

/// What the end of the chat draws: what the agent is still writing, the prompts it hasn't read yet, what the
/// session's record says it's at, and where the pull request stands, which a line for a wait on it says.
pub(in super::super) struct EndOfChat<'a> {
    pub(in super::super) tail: &'a Tail,
    pub(in super::super) waiting: &'a [Waiting],
    pub(in super::super) at_work: &'a AtWork,
    /// What's open, which says how a thought still being written shows, and which lines for several background tasks
    /// have theirs under them.
    pub(in super::super) folds: &'a Folds,
    pub(in super::super) pull_request: Option<&'a PullRequestStanding>,
}

/// A prompt the agent hasn't read yet, which is drawn after what it's still writing, as it will be once it's read.
#[expect(clippy::large_enum_variant, reason = "made for one frame's tail and drawn straight away, as a row is")]
pub(in super::super) enum Waiting {
    /// Recorded, with what drawing it needs.
    Recorded(SessionEvent, Around),
    /// On its way to the hub, with the files it carries, whose images are already here.
    Sending(QueuedPrompt, Vec<Carried>),
}

/// The prompts the agent hasn't read yet, oldest first: those recorded, then those on their way.
pub(in super::super) fn waiting(log: &SessionLog, m: &Model) -> Vec<Waiting> {
    let recorded = log
        .waiting_prompts()
        .into_iter()
        .map(|prompt| Waiting::Recorded(prompt.clone(), Around::read(log, m, prompt.seq, &prompt.body)));
    let sending =
        log.pending().iter().map(|sent| Waiting::Sending(sent.clone(), attachments::carried(m, &sent.prompt.files)));
    recorded.chain(sending).collect()
}

/// Text the agent is still writing, then the prompts it hasn't read yet, then the working line while it works, then
/// what it's running in the background, then its wait on the pull request. A thought shows its latest line, or all of
/// it when thinking starts open, the way it will once it's recorded. A call shows once it's recorded: until then it isn't
/// running, and the working line says the agent is at work. A subagent's line opens the subagent tab when the log is
/// read `by` place, which keeps its work apart, since only then can the tab read it. Nothing's found at the end of the
/// chat, since it isn't counted, so nothing's marked there either.
pub(in super::super) fn render_tail(end: &EndOfChat, shared: &Shared, by: By, cx: &App) -> AnyElement {
    let EndOfChat { tail, waiting, at_work, folds, pull_request } = *end;
    let (written, placed) = still_writing(tail, shared, folds.thinking_open(), cx);
    let mut after: Vec<AnyElement> = Vec::new();
    if at_work.working {
        let line = working_line(at_work.turn_started_at_ms, tail.compacting.as_ref(), &shared.line_text, cx);
        after.push(line.into_any_element());
    }
    // Left going by a turn, or started during one.
    after.extend(background::lines(&at_work.background, folds, by, shared, cx));
    let wait = at_work.wait.iter().map(|wait| background::wait_line(wait, pull_request, shared, cx));
    after.extend(wait.map(IntoElement::into_any_element));
    if waiting.is_empty() {
        if written.is_empty() && after.is_empty() {
            return empty();
        }
        return row().children(written).children(after).into_any_element();
    }
    // Each prompt is a row of its own, as it will be once it's read, between what the agent is writing and the
    // working line, which stays the chat's last.
    let prompts = (placed..).zip(waiting).map(|(n, prompt)| waiting_prompt(prompt, TextAt::at_the_end(n), shared, cx));
    v_flex()
        .w_full()
        .when(!written.is_empty(), |d| d.child(row().children(written)))
        .children(prompts)
        .when(!after.is_empty(), |d| d.child(row().children(after)))
        .into_any_element()
}

/// What the agent is still writing, block by block, and how many texts that is, each at the next place at the end of
/// the chat. Each is known as it will be once it's recorded. A subagent's work shows in the call that started it, once
/// it's recorded.
fn still_writing(tail: &Tail, shared: &Shared, thinking_open: bool, cx: &App) -> (Vec<AnyElement>, usize) {
    let (mut out, mut placed) = (Vec::new(), 0);
    for message in tail.messages.iter().filter(|m| m.parent_tool_use_id.is_none()) {
        for block in &message.blocks {
            let key = ItemKey::Block(message.message_id.clone(), block.index);
            let mut drawn = |part| {
                let (element, at) = (find::view_id(&(key.clone(), part)), TextAt::at_the_end(placed));
                placed += 1;
                from_blocks(&Written::unfound(at, element, &block.text), None, shared)
            };
            match block.kind {
                BlockKind::Text => out.push(drawn(Part::Text)),
                BlockKind::Thinking if thinking_open => {
                    let thought_drawn = drawn(Part::Thinking);
                    out.push(thought("Thinking…".to_string(), cx).into_any_element());
                    out.push(thinking::text(&key, thought_drawn, cx).into_any_element());
                }
                BlockKind::Thinking => {
                    out.push(thought(format!("Thinking… {}", last_line(&block.text)), cx).into_any_element());
                }
                BlockKind::ToolUse | BlockKind::Unknown => {}
            }
        }
    }
    (out, placed)
}

/// A prompt the agent hasn't read yet, at `at`, drawn as it will be once it's read, faint until then.
fn waiting_prompt(prompt: &Waiting, at: TextAt, shared: &Shared, cx: &App) -> AnyElement {
    let find = FindLook::default();
    match prompt {
        Waiting::Recorded(event, around) => render_event(event, around, at, shared, &find, cx),
        Waiting::Sending(sent, carried) => {
            let key = ItemKey::Prompt(sent.client_msg_id.clone());
            let text = Prompt { text: &sent.prompt.text, find: &find, at };
            prompt_bubble(&key, text, carried, None, shared, cx)
        }
    }
}

/// A line as tall as a call's, at body size, so nothing jumps when what's above it is recorded.
pub(super) fn tail_line(cx: &App) -> Div {
    div().flex().items_center().gap_2().h(px(HEIGHT)).text_color(look::colors(cx).text_muted)
}

/// What a thought the agent is still writing says, after the icon its line will have once it's recorded. The working
/// line under it has the mark that says the agent is at work.
fn thought(text: String, cx: &App) -> Div {
    tail_line(cx).child(still_icon(thinking::ICON, cx)).child(clipped(text))
}

pub(super) fn clipped(text: String) -> Div {
    div().overflow_hidden().whitespace_nowrap().text_ellipsis().child(text)
}

/// The mark with its light sweeping, and how long the turn has taken so far, in the code font so the digits stay put.
/// While the agent compacts its conversation, it says so after the time, and how much of the summary it has written
/// when it says, since that's the only sign a compaction is getting on. The time and the words are on one baseline,
/// as a call's title and its code are.
fn working_line(
    started_at_ms: Option<i64>,
    compacting: Option<&Compacting>,
    line_text: &LineText,
    cx: &App,
) -> impl IntoElement {
    let so_far = started_at_ms.map(|started| {
        let elapsed = look::now_ms(cx) - started;
        // The hub's clock can be a little ahead of this one.
        turn_times::so_far(Duration::from_millis(u64::try_from(elapsed).unwrap_or(0)))
    });
    tail_line(cx)
        .id("working-line")
        .child(look::chat_mark(Signal::Working, look::Size::Inline, cx))
        .children(so_far.map(|so_far| line_text.code.set(div().flex_none().child(so_far))))
        .children(compacting.map(|compacting| {
            // Counted as tokens are, since it's said beside them.
            let written = compacting.written_chars.filter(|chars| *chars > 0);
            let written = written.map(|chars| format!(" · {} characters written", look::tokens(chars)));
            // Cut off where the line ends, rather than running past the pane.
            line_text.body.set(clipped(format!("Compacting the conversation{}", written.unwrap_or_default())).min_w_0())
        }))
        .test_support()
}

fn last_line(text: &str) -> String {
    let line = text.lines().last().unwrap_or("");
    line.chars().rev().take(120).collect::<Vec<_>>().into_iter().rev().collect()
}
