//! The lines at the end of the chat for what the agent has in the background: one for each task, with its ring, or a
//! subagent's robot and the button its call's line has, and past what there's room for, one for each group with
//! several, which opens to its tasks, as `crate::task_lines` says. Then one for each open wait on pull requests, with Stop.

use client_core::calls::UNDESCRIBED;
use client_core::tasks::Says;
use client_core::tool_servers::ToolServerNames;
use client_core::waits;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, WeakEntity, div,
};
use look::Tooltipped as _;
use look::{Moving, TaskRing};
use protocol::event::AgentTask;
use protocol::model::{WaitId, WaitUntil};

use super::{clipped, tail_line};
use crate::folds::{Fold, Folds};
use crate::rows::Shared;
use crate::rows::line::{START_CHARS, lead, opening};
use crate::rows::tools::agent::{Opens, open_tab, tab_title};
use crate::task_lines::{TaskGroup, TaskLine, task_lines};
use crate::transcript::OpenWait;
use crate::transcript::Transcript;

/// The lines for the tasks the agent runs in the background, as `crate::task_lines` lists them.
pub(super) fn lines(
    tasks: &[AgentTask],
    folds: &Folds,
    names: &ToolServerNames,
    shared: &Shared,
    cx: &App,
) -> Vec<AnyElement> {
    let line = |line| match line {
        TaskLine::Task(group, task) => going(group, task, names, shared, cx).into_any_element(),
        TaskLine::Several { group, tasks, open } => several(group, &tasks, open, names, shared, cx),
        // Set in by a mark's width and the line's own gap, so its mark starts where the group's words do.
        TaskLine::Under(group, task) => tail_line(cx)
            .child(div().flex_none().w(look::Size::Inline.width()))
            .child(going(group, task, names, shared, cx).flex_1().min_w_0())
            .into_any_element(),
    };
    task_lines(tasks, folds).into_iter().map(line).collect()
}

/// A group's mark, which each of its tasks' lines starts with, and what pointing at one of them says. The ring or
/// the robot says it's in the background, so a line's words don't. The fan is the agent's, so no task has one.
fn mark(group: TaskGroup, shared: &Shared, cx: &App) -> (AnyElement, &'static str) {
    match group {
        TaskGroup::Subagents => (shared.moving.room(Moving::Robot).into_any_element(), "Running in the background"),
        TaskGroup::Commands => {
            (shared.moving.room(Moving::TaskRing(TaskRing::Running)).into_any_element(), "Running in the background")
        }
        TaskGroup::Monitors => (ring(TaskRing::Watching, cx), "Watching in the background"),
    }
}

/// One line for several tasks of a group: the mark each of them has, so the line says as much as theirs would about
/// whether anything is running, how many there are, and while it's closed, what each is for, fainter, as far as the
/// line has room. Clicking it opens it to their own lines, and closes it again, as a call's line does.
fn several(
    group: TaskGroup,
    tasks: &[&AgentTask],
    open: bool,
    names: &ToolServerNames,
    shared: &Shared,
    cx: &App,
) -> AnyElement {
    let colors = look::colors(cx);
    let (mark, pointed_at) = mark(group, shared, cx);
    let count = format!("{} {}", tasks.len(), group.name());
    let this = shared.this.clone();
    let set_open = move |cx: &mut App| {
        let _ = this.update(cx, |t, cx| t.set_open_at_end(group.key(), Fold::Tasks, !open, cx));
    };
    opening(group.key().element("several"), count.clone().into(), open, set_open, cx)
        .text_color(colors.text_muted)
        .says(pointed_at, cx)
        .child(lead(mark, look::Size::Inline.width(), open, cx))
        .child(div().flex_none().child(count))
        .when(!open, |line| line.child(clipped(what_for(tasks, names)).min_w_0().text_color(colors.text_faint)))
        .test_support()
        .into_any_element()
}

/// What each of `tasks` is for, one after another, as far as a line could have room for.
fn what_for(tasks: &[&AgentTask], names: &ToolServerNames) -> String {
    let mut said = String::new();
    for what in tasks.iter().filter_map(|task| Says::of(task, names)).map(Says::into_words) {
        if said.len() >= START_CHARS {
            break;
        }
        if !said.is_empty() {
            said.push_str(" \u{b7} ");
        }
        said.push_str(&what);
    }
    said
}

/// A background task's line: its group's mark, and what it's for, or else the command it runs, as that command's own
/// line says it. A subagent has the robot its call's
/// line has, so its line reads as that call, and it ends with the button that opens the subagent tab, as its call's
/// line does, once the agent has said which subagent it is, or which call started it.
fn going(group: TaskGroup, task: &AgentTask, names: &ToolServerNames, shared: &Shared, cx: &App) -> Div {
    let (mark, pointed_at) = mark(group, shared, cx);
    let open = Opens::task(task).map(|opens| {
        let id = ElementId::Name(format!("background-open/{}", task.id).into());
        open_tab(id, opens, &tab_title(task, names), &shared.this)
    });
    let says = Says::of(task, names).unwrap_or_else(|| Says::For(group.one().to_string()));
    in_the_background(("background", task.id.as_str()), mark, says, pointed_at, open, cx)
}

/// The agent's wait on pull requests, as the hub keeps it: the ring a monitor has, held still, since nothing runs for
/// it here, what it waits on, and where its pull requests stand, as the hub last read them, then Stop, which calls it
/// off. Pointing at it says what ends it.
pub(super) fn wait_line(open: &OpenWait, shared: &Shared, cx: &App) -> Div {
    let (text, pointed_at) = wait_words(open);
    let wait = &open.wait;
    let stop = stop_waiting(&wait.id, &shared.this);
    let mark = ring(TaskRing::Watching, cx);
    in_the_background(("wait", wait.id.as_str()), mark, Says::For(text), pointed_at, Some(stop), cx)
}

/// What a wait's line says, and what pointing at it says: what it waits on, and where its pull requests stand, from
/// the hub's reading of them. A wait with no reading says only what it waits on. That's any wait until the hub has said
/// what it read, and one the singular tool started, which nothing reads, and which is open only where an earlier hub's
/// calling it off failed.
fn wait_words(OpenWait { wait, read }: &OpenWait) -> (String, &'static str) {
    let pointed_at = match (read, wait.until) {
        (Some(_), _) => {
            "Until every watched pull request reaches the condition, or one needs the agent; Stop ends only this wait"
        }
        (None, WaitUntil::Checks | WaitUntil::RequiredChecks | WaitUntil::AllChecks) => {
            "Until its checks have run and none failed, or it needs the agent"
        }
        (None, WaitUntil::Done | WaitUntil::Merged | WaitUntil::Unknown) => {
            "Until it's merged or closed, or needs the agent"
        }
    };
    (waits::waiting(wait, read.as_ref()), pointed_at)
}

/// Calls off the wait. Its line goes as the hub records that, and the agent's CLI hears as it next asks after its task,
/// which then tells the agent, or with no agent running, it hears with the next prompt.
fn stop_waiting(wait: &WaitId, this: &WeakEntity<Transcript>) -> AnyElement {
    let (this, wait) = (this.clone(), wait.clone());
    look::button(
        ElementId::Name(format!("wait-stop/{wait}").into()),
        look::ButtonKind::Outlined,
        look::ControlSize::Small,
        "Stop",
    )
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
    says: Says,
    pointed_at: &'static str,
    button: Option<AnyElement>,
    cx: &App,
) -> Div {
    let words = match &says {
        Says::For(what) => clipped(what.clone()),
        Says::Runs(command) => div()
            .flex()
            .items_center()
            .min_w_0()
            .gap(look::Space::Small)
            .child(div().flex_none().child(UNDESCRIBED))
            .child(look::pill(look::PillKind::Plain, command.clone())),
    };
    let described = div()
        .id(ElementId::Name(format!("{kind}-{key}").into()))
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap(look::Space::Small)
        .child(lead)
        .child(
            words.id(ElementId::Name(format!("{kind}-text/{key}").into())).aria_label(says.into_words()).test_support(),
        )
        .says(pointed_at, cx)
        .test_support();
    tail_line(cx).child(described).when_some(button, |line, button| line.child(div().flex_none().child(button)))
}

fn ring(ring: TaskRing, cx: &App) -> AnyElement {
    look::task_ring(ring, cx).into_any_element()
}
