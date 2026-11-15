//! What a turn the agent started by itself took in: a line for each of its background tasks it heard had ended, before
//! what it did about them, so the chat says why the agent picked up with nobody asking. Each says what the task was
//! for, how it ended and when, after the icon its call has, but for a wait's, which has a pull request's, since its
//! call's plug says nothing of what it waited on. A subagent's has the button its call's line has, which opens its work
//! in the subagent tab.

use client_core::calls::agent::UNTITLED;
use client_core::waits::{watched, why_it_ended};
use gpui_kit::assets::IconName;
use gpui_kit::{AnyElement, App, IntoElement as _, ParentElement as _, Styled as _};
use protocol::event::{EndedTask, SessionEvent, TaskKind, TaskOutcome};

use super::line::{Line, Part};
use super::tools::{agent, command, typed};
use super::{Shared, empty, row};
use crate::item_key::ItemKey;
use crate::task_lines::TaskGroup;

/// The lines for `tasks`, which the turn the agent started by itself at `event` took in, one under another as calls'
/// lines are, each at the time the turn started, since that's when the agent heard.
pub(super) fn lines(event: &SessionEvent, tasks: &[EndedTask], shared: &Shared, cx: &App) -> AnyElement {
    if tasks.is_empty() {
        return empty();
    }
    let at = look::day_and_time(event.ts_ms, cx);
    let lines = tasks.iter().enumerate().map(|(ix, ended)| {
        let (icon, title, how) = words(ended);
        let note = at.as_ref().map_or_else(|| how.to_string(), |at| format!("{how} \u{b7} {at}"));
        let label = at.as_ref().map_or_else(|| format!("{title}, {how}"), |at| format!("{title}, {how}, {at}"));
        let task = &ended.task;
        let open = task.call.as_ref().filter(|_| task.kind == TaskKind::Subagent).map(|call| {
            let id = ItemKey::Event(event.seq, ix).element("ended-task-open");
            agent::open_tab(id, call.clone(), task.description.as_deref().unwrap_or(UNTITLED), &shared.this)
        });
        let line = Line::new(icon, title).part(Part::Note(note.into()));
        let line = match open {
            Some(open) => line.action(open),
            None => line,
        };
        line.still(ItemKey::Event(event.seq, ix).element("ended-task"), label, &shared.line_text, cx)
    });
    // Spaced as a message of only calls is, since they read as calls' lines.
    row().pb(look::Space::Small).children(lines).into_any_element()
}

/// A task's icon, what it says the task was, and how it ended. A wait says which pull requests it waited on, and why it
/// ended, from the hub's record of it, since the agent's word for what a wait is for is its tool's name, and for how it
/// ended, only that its tool answered.
fn words(ended: &EndedTask) -> (IconName, String, &'static str) {
    let (task, how) = (&ended.task, how_it_ended(ended.outcome));
    let (icon, group) = match task.kind {
        TaskKind::Subagent => (agent::ICON, TaskGroup::Subagents),
        TaskKind::Command => (command::ICON, TaskGroup::Commands),
        TaskKind::Monitor => (typed::ICON, TaskGroup::Monitors),
        TaskKind::Wait => {
            let watching = ended.wait.as_ref().map(|wait| (watched(&wait.watching), wait.ended));
            let (what, how) = match watching {
                Some((what, ended)) => (what, ended.map_or(how, |ended| why_it_ended(ended.how))),
                None => ("a pull request".to_string(), how),
            };
            return (IconName::GitPullRequest, format!("Waited on {what}"), how);
        }
    };
    (icon, task.description.clone().unwrap_or_else(|| group.one().to_string()), how)
}

fn how_it_ended(outcome: TaskOutcome) -> &'static str {
    match outcome {
        TaskOutcome::Finished => "Finished",
        TaskOutcome::Failed => "Failed",
        TaskOutcome::Stopped => "Stopped",
        TaskOutcome::Unknown => "Ended",
    }
}

#[cfg(test)]
mod tests {
    use protocol::event::{AgentTask, EndedTask, TaskKind, TaskOutcome};
    use protocol::model::{EndedWait, WaitEnd, WaitEnded};

    use super::words;

    fn ended(kind: TaskKind, description: Option<&str>, outcome: TaskOutcome) -> EndedTask {
        let task = AgentTask {
            id: "t1".into(),
            description: description.map(str::to_string),
            kind,
            call: Some("toolu-1".into()),
        };
        EndedTask { task, outcome, wait: None }
    }

    #[test]
    fn a_tasks_line_says_what_the_agent_said_its_for_and_how_it_ended() {
        let said = |task: &EndedTask| {
            let (_, title, how) = words(task);
            (title, how)
        };
        let review = ended(TaskKind::Subagent, Some("Review the depot's design"), TaskOutcome::Stopped);
        assert_eq!(said(&review), ("Review the depot's design".to_string(), "Stopped"));
        let tests = ended(TaskKind::Command, None, TaskOutcome::Failed);
        assert_eq!(said(&tests), ("A command".to_string(), "Failed"), "what one says the agent didn't describe");
        let watch = ended(TaskKind::Monitor, Some("Watch the run for failures"), TaskOutcome::Unknown);
        let unknown = ("Watch the run for failures".to_string(), "Ended");
        assert_eq!(said(&watch), unknown, "an outcome this build doesn't know");
    }

    /// The agent describes a wait by its tool's name, and hears only that its tool answered, so its line says what the
    /// hub keeps of it.
    #[test]
    fn a_waits_line_says_what_it_waited_on_and_why_it_ended_from_the_hubs_record() {
        let mut wait = protocol::fixtures::pull_request_wait("w1");
        let mut task = ended(TaskKind::Wait, Some("roundhouse/wait_for_pull_requests"), TaskOutcome::Finished);
        assert_eq!(words(&task).1, "Waited on a pull request", "without the hub's record");

        wait.ended = Some(WaitEnded { at_ms: 2, how: WaitEnd::ChecksFailed });
        task.wait = Some(EndedWait::from(&wait));
        let (_, title, how) = words(&task);
        assert_eq!((title.as_str(), how), ("Waited on #7: required checks", "A check failed"));

        wait.ended = None;
        task.wait = Some(EndedWait::from(&wait));
        assert_eq!(words(&task).2, "Finished", "a record that doesn't say how it ended");
    }
}
