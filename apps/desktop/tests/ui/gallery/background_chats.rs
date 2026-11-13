//! Chats with more in the background than has a line each, and the screens about how the end of the chat lists them,
//! which the gallery takes after `subagent_chats.rs`'s. The same made-up railway as the other chats.

#![cfg(test)]

use protocol::event::{AgentTask, EventBody, TaskKind};
use protocol::model::{Seq, SessionStatus};
use serde_json::json;

use super::events::{replied, subagent_started, taken, task, text, tool_result, tool_use, user_prompt};
use super::frame::{CHAT, Frame, Part};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::subagent_chats::LAUNCHED;
use super::world::World;
use super::worlds::chat_world;

/// Several tasks' lines, from the first of them to the last, without the calls above that started them.
const LINES: Frame = Frame::Around(Part::Line("6 subagents"), &[Part::Line("2 monitors")]);

/// The screens about more in the background than has a line each.
pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "background-five",
        spec: "chats.md",
        world: Some(|| left_going(&[(TaskKind::Subagent, 3), (TaskKind::Command, 1), (TaskKind::Monitor, 1)])),
        frame: CHAT,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "background-many",
        spec: "chats.md",
        world: Some(|| left_going(MANY)),
        frame: LINES,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "background-many-open",
        spec: "chats.md",
        world: Some(|| left_going(MANY)),
        frame: LINES,
        steps: &[Step::Select(FOCUS), Step::Open("6 subagents")],
    },
];

/// As many as a long piece of work leaves going: six subagents, five commands and two monitors.
const MANY: &[(TaskKind, usize)] = &[(TaskKind::Subagent, 6), (TaskKind::Command, 5), (TaskKind::Monitor, 2)];

/// The crates the made-up refactor goes through, each of which has a subagent.
const CRATES: [&str; 6] = ["Signals", "Depot", "Timetable", "Trains", "Points", "Platforms"];

/// What each command left running is for, and the command.
const COMMANDS: [(&str, &str); 5] = [
    ("Run the full test suite", "cargo test --workspace"),
    ("Wait for checks on the four pull requests", "tools/wait-for-checks.sh 201 202 203 204"),
    ("Poll the merge queue until both merge or one stops", "tools/poll-queue.sh 201 202"),
    ("Wait for the depot's pull request to merge or close", "gh pr view 202 --json state --watch"),
    ("Wait again for checks on the three stacked pull requests", "tools/wait-for-checks.sh 205 206 207"),
];

/// What each monitor watches for, and its command.
const MONITORS: [(&str, &str); 3] = [
    ("Watch the run for failures", "tail -f target/test.log | grep --line-buffered FAILED"),
    ("Watch the merge queue for a pull request leaving it", "tools/watch-queue.sh --left"),
    ("Watch the hub's log for a worker restarting", "tail -f hub.log | grep --line-buffered 'worker started'"),
];

/// The call that starts the `at`th task of a kind, by `call`, what the agent hears back at once, and the task as the
/// hub lists it.
fn starting(kind: TaskKind, at: usize, call: &str) -> (Vec<EventBody>, AgentTask) {
    let message = format!("msg-{call}");
    let (what_for, events) = match kind {
        TaskKind::Subagent => {
            let of = CRATES[at % CRATES.len()];
            let what_for = format!("{of}: typed aspects");
            let prompt = format!("Move the {of} crate over to typed aspects, and open a pull request for it.");
            let asked = json!({ "description": what_for, "subagent_type": "general-purpose", "prompt": prompt });
            let events = vec![
                tool_use(&message, call, "Agent", asked),
                subagent_started(call, "general-purpose", &what_for, &prompt),
                tool_result(call, LAUNCHED),
            ];
            (what_for, events)
        }
        TaskKind::Monitor => {
            let (what_for, command) = MONITORS[at % MONITORS.len()];
            let asked = json!({ "command": command, "description": what_for, "timeout_ms": 600_000 });
            let heard = "Monitor started. You will be notified on each event.";
            (what_for.to_string(), vec![tool_use(&message, call, "Monitor", asked), tool_result(call, heard)])
        }
        TaskKind::Command | TaskKind::Wait => {
            let (what_for, command) = COMMANDS[at % COMMANDS.len()];
            let asked = json!({ "command": command, "description": what_for, "run_in_background": true });
            let heard = "Command running in background.";
            (what_for.to_string(), vec![tool_use(&message, call, "Bash", asked), tool_result(call, heard)])
        }
    };
    (events, task(kind, call, &what_for))
}

/// The start of a turn that left `count` tasks of each kind going in the background, in the order given, each
/// started by a call of its own, and then the list of them all.
fn started(kinds: &[(TaskKind, usize)]) -> Vec<EventBody> {
    let mut events = vec![
        user_prompt("Move every crate over to typed aspects, a pull request for each, and get them merged."),
        taken(Seq::new(1)),
        text("msg-1", "I'll give each crate to an agent of its own, and watch their pull requests through the queue."),
    ];
    let mut tasks = Vec::new();
    for (kind, count) in kinds {
        for at in 0..*count {
            let (calls, task) = starting(*kind, at, &format!("toolu-{}", tasks.len() + 1));
            events.extend(calls);
            tasks.push(task);
        }
    }
    events.push(EventBody::AgentTasks { tasks, stopped: Vec::new() });
    events
}

/// A turn that's over with `kinds` still going in the background.
fn left_going(kinds: &[(TaskKind, usize)]) -> World {
    let reply = "They're all at work. I'll pick up again as each one reports, or sooner if a check fails.";
    let mut events = started(kinds);
    events.extend([text("msg-2", reply), replied(reply, 48, 0.37)]);
    chat_world(SessionStatus::Background, events)
}
