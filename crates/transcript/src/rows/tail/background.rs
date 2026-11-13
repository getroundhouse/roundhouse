//! The lines at the end of the chat for what the agent has in the background: one for each task, with its ring, or a
//! subagent's robot and the button its call's line has, and past what there's room for, one for each group with
//! several, which opens to its tasks, as `crate::task_lines` says. Then one for the wait it has open on the pull
//! request, with Stop.

use client_core::session_log::By;
use gpui_kit::component::button::Button;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, WeakEntity, div,
};
use look::{Hand as _, Sizing as _, TaskRing};
use protocol::event::AgentTask;
use protocol::model::{Wait, WaitId, WaitUntil};

use super::{clipped, tail_line};
use crate::folds::{Fold, Folds};
use crate::rows::Shared;
use crate::rows::line::{START_CHARS, lead, opening};
use crate::rows::tools::agent::open_tab;
use crate::task_lines::{TaskGroup, TaskLine, task_lines};
use crate::transcript::{PullRequestStanding, Transcript};
use client_core::calls::agent;

/// The lines for the tasks the agent runs in the background, as `crate::task_lines` lists them.
pub(super) fn lines(tasks: &[AgentTask], folds: &Folds, by: By, shared: &Shared, cx: &App) -> Vec<AnyElement> {
    let line = |line| match line {
        TaskLine::Task(group, task) => going(group, task, by, shared, cx).into_any_element(),
        TaskLine::Several { group, tasks, open } => several(group, &tasks, open, shared, cx),
        // Set in by a mark's width and the line's own gap, so its mark starts where the group's words do.
        TaskLine::Under(group, task) => tail_line(cx)
            .child(div().flex_none().w(look::Size::Inline.width()))
            .child(going(group, task, by, shared, cx).flex_1().min_w_0())
            .into_any_element(),
    };
    task_lines(tasks, folds).into_iter().map(line).collect()
}

/// A group's mark, which each of its tasks' lines starts with, and what pointing at one of them says. The ring or
/// the robot says it's in the background, so a line's words don't. The fan is the agent's, so no task has one.
fn mark(group: TaskGroup, cx: &App) -> (AnyElement, &'static str) {
    match group {
        TaskGroup::Subagents => (look::robot_at_work(cx).into_any_element(), "Running in the background"),
        TaskGroup::Commands => (ring(TaskRing::Running, cx), "Running in the background"),
        TaskGroup::Monitors => (ring(TaskRing::Watching, cx), "Watching in the background"),
    }
}

/// One line for several tasks of a group: the mark each of them has, so the line says as much as theirs would about
/// whether anything is running, how many there are, and while it's closed, what each is for, fainter, as far as the
/// line has room. Clicking it opens it to their own lines, and closes it again, as a call's line does.
fn several(group: TaskGroup, tasks: &[&AgentTask], open: bool, shared: &Shared, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let (mark, pointed_at) = mark(group, cx);
    let count = format!("{} {}", tasks.len(), group.name());
    let this = shared.this.clone();
    let set_open = move |cx: &mut App| {
        let _ = this.update(cx, |t, cx| t.set_open_at_end(group.key(), Fold::Tasks, !open, cx));
    };
    opening(group.key().element("several"), count.clone().into(), open, set_open, cx)
        .text_color(colors.text_muted)
        .tooltip(move |window, cx| Tooltip::element(move |_, _| said(pointed_at)).build(window, cx))
        .child(lead(mark, look::Size::Inline.width(), open, cx))
        .child(div().flex_none().child(count))
        .when(!open, |line| line.child(clipped(what_for(tasks)).min_w_0().text_color(colors.text_faint)))
        .test_support()
        .into_any_element()
}

/// What each of `tasks` is for, one after another, as far as a line could have room for.
fn what_for(tasks: &[&AgentTask]) -> String {
    let mut said = String::new();
    for what in tasks.iter().filter_map(|task| task.description.as_deref()) {
        if said.len() >= START_CHARS {
            break;
        }
        if !said.is_empty() {
            said.push_str(" \u{b7} ");
        }
        said.push_str(what);
    }
    said
}

/// A background task's line: its group's mark, and what the agent said it's for. A subagent has the robot its call's
/// line has, so its line reads as that call, and it ends with the button that opens the subagent tab, as its call's
/// line does, once the agent has said which call started it.
fn going(group: TaskGroup, task: &AgentTask, by: By, shared: &Shared, cx: &App) -> Div {
    let (mark, pointed_at) = mark(group, cx);
    let text = task.description.clone().unwrap_or_else(|| group.one().to_string());
    let open = task.call.as_ref().filter(|_| by == By::Place && group == TaskGroup::Subagents).map(|call| {
        let id = ElementId::Name(format!("background-open/{call}").into());
        open_tab(id, call.clone(), task.description.as_deref().unwrap_or(agent::UNTITLED), &shared.this)
    });
    in_the_background(("background", task.id.as_str()), mark, text, pointed_at, open, cx)
}

/// The agent's wait on the pull request, as the hub keeps it: the ring a monitor has, held still, since nothing runs
/// for it here, what it waits on, and where the pull request stands on GitHub, which the agent's own words for it
/// don't say, then Stop, which calls it off. Pointing at it says what ends it.
pub(super) fn wait_line(wait: &Wait, standing: Option<&PullRequestStanding>, shared: &Shared, cx: &App) -> Div {
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

/// Calls off the wait. Its line goes as the hub records that, and the agent's CLI hears as it next asks after its task,
/// which then tells the agent, or with no agent running, it hears with the next prompt.
fn stop_waiting(wait: &WaitId, this: &WeakEntity<Transcript>) -> AnyElement {
    let (this, wait) = (this.clone(), wait.clone());
    Button::new(ElementId::Name(format!("wait-stop/{wait}").into()))
        .hand()
        .outline()
        .sized(look::ControlSize::Small)
        .label("Stop")
        .tooltip("Stop waiting, and tell the agent")
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let _ = this.update(cx, |t, cx| t.call_off_wait(wait.clone(), cx));
        })
        .into_any_element()
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

fn ring(ring: TaskRing, cx: &App) -> AnyElement {
    look::task_ring(ring, cx).into_any_element()
}

/// What pointing at a background task's line says.
fn said(words: &'static str) -> impl IntoElement {
    div().id("background-pointed-at").aria_label(words).child(words).test_support()
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
