//! The end of the transcript: text the agent is still writing, the prompts it hasn't read yet, the working line while
//! it works, a line for each task it's running in the background, which for a subagent has the robot and the button
//! its call's line has, and one for the wait it has open on the pull request, with Stop.

use std::time::Duration;

use client_core::calls::agent;
use client_core::session_log::{By, SessionLog};
use gpui_kit::component::button::Button;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, WeakEntity, div, px,
};
use look::{Hand as _, LineText, Signal, TaskRing};
use protocol::event::{AgentTask, SessionEvent, TaskKind};
use protocol::model::{QueuedPrompt, Wait, WaitId, WaitUntil};
use protocol::tail::{BlockKind, Compacting, Tail};

use super::around::Around;
use super::line::{HEIGHT, still_icon};
use super::prompt::{Progress, Prompt, prompt_bubble};
use super::tools::agent::open_tab;
use super::{Shared, empty, render_event, row, thinking, turn_times};
use crate::attachments::{self, Carried};
use crate::find::{FindLook, Part};
use crate::item_key::ItemKey;
use crate::transcript::{AtWork, Model, PullRequestStanding, Transcript};

/// What the end of the chat draws: what the agent is still writing, the prompts it hasn't read yet, what the
/// session's record says it's at, and where the pull request stands, which a line for a wait on it says.
pub(super) struct EndOfChat<'a> {
    pub(super) tail: &'a Tail,
    pub(super) waiting: &'a [Waiting],
    pub(super) at_work: &'a AtWork,
    pub(super) pull_request: Option<&'a PullRequestStanding>,
}

/// A prompt the agent hasn't read yet, which is drawn after what it's still writing, as it will be once it's read.
#[expect(clippy::large_enum_variant, reason = "made for one frame's tail and drawn straight away, as a row is")]
pub(super) enum Waiting {
    /// Recorded, with what drawing it needs.
    Recorded(SessionEvent, Around),
    /// On its way to the hub, with the files it carries, whose images are already here.
    Sending(QueuedPrompt, Vec<Carried>),
}

/// The prompts the agent hasn't read yet, oldest first: those recorded, then those on their way.
pub(super) fn waiting(log: &SessionLog, m: &Model) -> Vec<Waiting> {
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
/// it when `thinking_open`, the way it will once it's recorded. A call shows once it's recorded: until then it isn't
/// running, and the working line says the agent is at work. A subagent's line opens the subagent tab when the log is
/// read `by` place, which keeps its work apart, since only then can the tab read it.
pub(super) fn render_tail(
    end: &EndOfChat,
    shared: &Shared,
    by: By,
    thinking_open: bool,
    find: &FindLook,
    cx: &App,
) -> AnyElement {
    let EndOfChat { tail, waiting, at_work, pull_request } = *end;
    let written = still_writing(tail, shared, thinking_open, cx);
    let mut after: Vec<AnyElement> = Vec::new();
    if at_work.working {
        let line = working_line(at_work.turn_started_at_ms, tail.compacting.as_ref(), &shared.line_text, cx);
        after.push(line.into_any_element());
    }
    // Left going by a turn, or started during one.
    after.extend(at_work.background.iter().map(|task| going(task, by, shared, cx).into_any_element()));
    after.extend(at_work.wait.iter().map(|wait| wait_line(wait, pull_request, shared, cx).into_any_element()));
    if waiting.is_empty() {
        if written.is_empty() && after.is_empty() {
            return empty();
        }
        return row().children(written).children(after).into_any_element();
    }
    // Each prompt is a row of its own, as it will be once it's read, between what the agent is writing and the
    // working line, which stays the chat's last.
    let prompts = waiting.iter().map(|prompt| waiting_prompt(prompt, shared, find, cx));
    v_flex()
        .w_full()
        .when(!written.is_empty(), |d| d.child(row().children(written)))
        .children(prompts)
        .when(!after.is_empty(), |d| d.child(row().children(after)))
        .into_any_element()
}

/// What the agent is still writing, block by block. A subagent's work shows in the call that started it, once it's
/// recorded.
fn still_writing(tail: &Tail, shared: &Shared, thinking_open: bool, cx: &App) -> Vec<AnyElement> {
    let texts = &shared.texts;
    let mut out = Vec::new();
    for message in tail.messages.iter().filter(|m| m.parent_tool_use_id.is_none()) {
        for block in &message.blocks {
            let key = ItemKey::Block(message.message_id.clone(), block.index);
            match block.kind {
                BlockKind::Text => {
                    out.push(texts.streaming(&(key.clone(), Part::Text), &block.text).into_any_element());
                }
                BlockKind::Thinking if thinking_open => {
                    let text = texts.streaming(&(key.clone(), Part::Thinking), &block.text);
                    out.push(thought("Thinking…".to_string(), cx).into_any_element());
                    out.push(thinking::text(&key, text, cx).into_any_element());
                }
                BlockKind::Thinking => {
                    out.push(thought(format!("Thinking… {}", last_line(&block.text)), cx).into_any_element());
                }
                BlockKind::ToolUse | BlockKind::Unknown => {}
            }
        }
    }
    out
}

/// A prompt the agent hasn't read yet, drawn as it will be once it's read, faint until then.
fn waiting_prompt(prompt: &Waiting, shared: &Shared, find: &FindLook, cx: &App) -> AnyElement {
    match prompt {
        Waiting::Recorded(event, around) => render_event(event, around, None, shared, find, cx),
        Waiting::Sending(sent, carried) => {
            let key = ItemKey::Prompt(sent.client_msg_id.clone());
            let text = Prompt { text: &sent.prompt.text, find };
            prompt_bubble(&key, text, carried, Progress::Sending, shared, cx)
        }
    }
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
fn going(task: &AgentTask, by: By, shared: &Shared, cx: &App) -> Div {
    let (lead, what, pointed_at) = match task.kind {
        TaskKind::Command => (ring(TaskRing::Running, cx), "A command", "Running in the background"),
        TaskKind::Subagent => (look::robot_at_work(cx).into_any_element(), "A subagent", "Running in the background"),
        TaskKind::Monitor => (ring(TaskRing::Watching, cx), "A monitor", "Watching in the background"),
        // Its wait's line is drawn from the hub's wait, so its task isn't drawn.
        TaskKind::Wait => (ring(TaskRing::Watching, cx), "A wait", "Waiting in the background"),
    };
    let text = task.description.clone().unwrap_or_else(|| what.to_string());
    let open = task.call.as_ref().filter(|_| by == By::Place && task.kind == TaskKind::Subagent).map(|call| {
        let id = ElementId::Name(format!("background-open/{call}").into());
        open_tab(id, call.clone(), task.description.as_deref().unwrap_or(agent::UNTITLED), &shared.this)
    });
    in_the_background(("background", task.id.as_str()), lead, text, pointed_at, open, cx)
}

/// The agent's wait on the pull request, as the hub keeps it: the ring a monitor has, held still, since nothing runs
/// for it here, what it waits on, and where the pull request stands on GitHub, which the agent's own words for it
/// don't say, then Stop, which calls it off. Pointing at it says what ends it.
fn wait_line(wait: &Wait, standing: Option<&PullRequestStanding>, shared: &Shared, cx: &App) -> Div {
    let (text, pointed_at) = wait_words(wait, standing);
    let stop = stop_waiting(&wait.id, &shared.this);
    in_the_background(("wait", wait.id.as_str()), ring(TaskRing::Watching, cx), text, pointed_at, Some(stop), cx)
}

/// What a wait's line says, and what pointing at it says: what it waits on, and where the pull request stands, from
/// what's said of the one it waits on.
fn wait_words(wait: &Wait, standing: Option<&PullRequestStanding>) -> (String, &'static str) {
    let (what, pointed_at) = match wait.until {
        WaitUntil::Checks => (
            format!("Waiting on #{}'s checks", wait.number),
            "Until its checks have run and none failed, or it needs the agent",
        ),
        WaitUntil::Done | WaitUntil::Unknown => {
            (format!("Waiting on #{}", wait.number), "Until it's merged or closed, or needs the agent")
        }
    };
    // What's said of another pull request says nothing of this one, which the branch had as the wait started.
    let text = match standing.filter(|standing| standing.number == wait.number) {
        Some(standing) => format!("{what} \u{b7} {}", standing.words),
        None => what,
    };
    (text, pointed_at)
}

/// A line for what the agent has in the background, named `kind-key`, and its text `kind-text/key`: what leads it,
/// what it says, which pointing at it says more of, and the button it ends with. The button says what it does when
/// pointed at, so the rest of the line says where it runs, and gives way to the button, which stays at the far end.
fn in_the_background(
    (kind, key): (&str, &str),
    lead: AnyElement,
    text: String,
    pointed_at: &'static str,
    button: Option<AnyElement>,
    cx: &App,
) -> Div {
    let described = div()
        .id(ElementId::Name(format!("{kind}-{key}").into()))
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap_2()
        .child(lead)
        .child(
            clipped(text.clone())
                .id(ElementId::Name(format!("{kind}-text/{key}").into()))
                .aria_label(text)
                .test_support(),
        )
        .tooltip(move |window, cx| Tooltip::element(move |_, _| said(pointed_at)).build(window, cx))
        .test_support();
    tail_line(cx).child(described).when_some(button, |line, button| line.child(div().flex_none().child(button)))
}

/// Calls off the wait. Its line goes as the hub records that, and the agent's CLI hears as it next asks after its task,
/// which then tells the agent, or with no agent running, it hears with the next prompt.
fn stop_waiting(wait: &WaitId, this: &WeakEntity<Transcript>) -> AnyElement {
    let (this, wait) = (this.clone(), wait.clone());
    Button::new(ElementId::Name(format!("wait-stop/{wait}").into()))
        .hand()
        .outline()
        .xsmall()
        .label("Stop")
        .tooltip("Stop waiting, and tell the agent")
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let _ = this.update(cx, |t, cx| t.call_off_wait(wait.clone(), cx));
        })
        .into_any_element()
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

#[cfg(test)]
mod tests {
    use protocol::model::{Wait, WaitUntil};

    use super::wait_words;
    use crate::transcript::PullRequestStanding;

    /// The branch's pull request can be another by the time the app reads it, which says nothing of the one waited on.
    #[test]
    fn a_waits_line_says_where_only_its_own_pull_request_stands() {
        let wait = Wait {
            id: "w1".into(),
            session_id: "s1".into(),
            workspace_id: "ws1".into(),
            number: 7,
            call: None,
            until: WaitUntil::Checks,
            started_at_ms: 0,
            ended: None,
        };
        let standing = |number| PullRequestStanding { number, words: "2 checks running".to_string() };
        let said = |wait: &Wait, standing: Option<&PullRequestStanding>| wait_words(wait, standing).0;

        assert_eq!(said(&wait, Some(&standing(7))), "Waiting on #7's checks \u{b7} 2 checks running");
        assert_eq!(said(&wait, Some(&standing(8))), "Waiting on #7's checks", "another pull request's");
        assert_eq!(said(&wait, None), "Waiting on #7's checks");
        let done = Wait { until: WaitUntil::Done, ..wait.clone() };
        assert_eq!(said(&done, Some(&standing(7))), "Waiting on #7 \u{b7} 2 checks running");
    }
}
