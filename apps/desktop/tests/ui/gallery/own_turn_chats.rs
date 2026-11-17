//! A chat the agent picked up by itself, as a subagent and then a wait it left in the background ended, and the screen
//! of it, which the gallery takes after the background tasks'. The same made-up railway as the other chats.

#![cfg(test)]

use protocol::capability::turns::OwnTurn;
use protocol::event::{AgentTask, EndedTask, EventBody, TaskKind, TaskOutcome};
use protocol::model::{EndedWait, Seq, SessionStatus, WaitEnd, WaitEnded};
use serde_json::json;

use super::events::{replied, subagent_ended, subagent_started, taken, task, text, tool_result, tool_use, user_prompt};
use super::frame::CHAT;
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::subagent_chats::LAUNCHED;
use super::worlds::chat_world;

/// The screens about turns the agent started by itself.
pub(crate) const SCREENS: &[Screen] = &[Screen {
    name: "picked-up-by-itself",
    spec: "chats.md",
    world: Some(|| chat_world(SessionStatus::Idle, picked_up())),
    frame: CHAT,
    steps: &[Step::Select(FOCUS)],
}];

const REVIEW: &str = "Review the depot's design";
const REVIEW_PROMPT: &str = "Read crates/depot and say where its design lets a train leave before its signal clears.";
/// What the CLI calls a task a tool runs, which is the tool's name.
const WAIT_TOOL: &str = "roundhouse/wait_for_pull_requests";

/// A turn that left a review and a wait on #7's checks in the background, then a turn the agent started by itself as
/// the review finished, and another as the wait ended.
fn picked_up() -> Vec<EventBody> {
    let reply = "Both are going. I'll pick up as each one reports.";
    let review = json!({
        "description": REVIEW, "subagent_type": "general-purpose", "prompt": REVIEW_PROMPT, "run_in_background": true,
    });
    let wait_task = AgentTask {
        id: "w1".into(),
        description: Some(WAIT_TOOL.into()),
        kind: TaskKind::Wait,
        call: Some("toolu-2".into()),
        subagent: None,
    };
    let found = "The depot releases a train as soon as its route is set, before the signal clears.";
    let fixing = "The review found that the depot lets a train go before its signal clears. I'll fix that once #7 is in the queue.";
    let queued = "#7's required checks passed, so I've added it to the merge queue.";
    let mut wait = protocol::fixtures::pull_request_wait("toolu-2");
    wait.ended = Some(WaitEnded { at_ms: 2, how: WaitEnd::ChecksPassed });
    vec![
        user_prompt("Have an agent review the depot's design while #7's checks run, and queue #7 once they pass."),
        taken(Seq::new(1)),
        text("msg-1", "I'll have an agent review the depot, and wait on #7's checks meanwhile."),
        tool_use("msg-1", "toolu-1", "Agent", review),
        subagent_started("toolu-1", "general-purpose", REVIEW, REVIEW_PROMPT),
        tool_result("toolu-1", LAUNCHED),
        tool_use(
            "msg-2",
            "toolu-2",
            "mcp__roundhouse__wait_for_pull_requests",
            json!({ "pull_requests": [7], "until": "required_checks" }),
        ),
        tool_result("toolu-2", "The task w1 was moved to the background."),
        EventBody::AgentTasks {
            tasks: vec![task(TaskKind::Subagent, "toolu-1", REVIEW), wait_task.clone()],
            stopped: Vec::new(),
        },
        text("msg-3", reply),
        replied(reply, 21, 0.09),
        subagent_ended("toolu-1", found, 31_400),
        EventBody::AgentTasks { tasks: vec![wait_task.clone()], stopped: Vec::new() },
        own_turn(EndedTask {
            task: task(TaskKind::Subagent, "toolu-1", REVIEW),
            outcome: TaskOutcome::Finished,
            wait: None,
        }),
        text("msg-4", fixing),
        replied(fixing, 6, 0.03),
        EventBody::AgentTasks { tasks: Vec::new(), stopped: Vec::new() },
        own_turn(EndedTask { task: wait_task, outcome: TaskOutcome::Finished, wait: Some(EndedWait::from(&wait)) }),
        text("msg-5", queued),
        replied(queued, 8, 0.04),
    ]
}

/// A turn the agent started by itself to take in how `task` ended, as the hub records it.
fn own_turn(task: EndedTask) -> EventBody {
    EventBody::OwnTurn(OwnTurn::TaskEnded { ended: vec![task] })
}
