//! Chats whose agent's requests failed the same way in a row, each try started by its prompt, partway through a turn
//! or by the agent picking up by itself as a task in the background ended, and the screens of them, which the gallery
//! takes after the screen of one failure. The same made-up railway as the other chats.

#![cfg(test)]

use protocol::capability::calls::call::Runs;
use protocol::capability::turns::OwnTurn;
use protocol::event::{Again, EndedTask, EventBody, TaskKind, TaskOutcome};
use protocol::model::{EndedWait, Seq, SessionStatus, WaitEnd, WaitEnded};
use serde_json::json;

use super::failure_chats::{refused, stopped_short};
use super::frame::CHAT;
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::worlds::chat_world;
use client_core::fake_hub::events::user_prompt;
use client_core::fake_hub::events::{
    command, first_turn, in_background, launched, mcp, mcp_said, printed, replied, start_turn, subagent,
    subagent_ended, subagent_started, task, text,
};
use client_core::fake_hub::{NOW_MS, World};

/// The screens about failures in a row.
pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "request-failed-picking-up",
        spec: "chats.md",
        frame: CHAT,
        world: Some(|| failing(refused_picking_up())),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "request-failed-in-a-row",
        spec: "chats.md",
        frame: CHAT,
        world: Some(|| failing(refused_partway_then_twice())),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "request-failed-six-times",
        spec: "chats.md",
        frame: CHAT,
        world: Some(|| failing(refused_six_times())),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "request-failed-six-times-open",
        spec: "chats.md",
        frame: CHAT,
        world: Some(|| failing(refused_six_times())),
        steps: &[Step::Select(FOCUS), Step::Open("2 earlier")],
    },
];

const REVIEW: &str = "Review the depot's design";
const REVIEW_PROMPT: &str = "Read crates/depot and say where its design lets a train leave before its signal clears.";
const FOUND: &str = "The depot releases a train as soon as its route is set, before the signal clears.";
/// How far apart the agent picks up by itself, so each try's line says a time of its own.
const APART_MS: i64 = 60_000;

/// The chat, with each turn the agent started by itself a minute after the one before, which the hub says stopped
/// short on the newest failure, so its card offers to try again.
fn failing(events: Vec<EventBody>) -> World {
    let picked_up: Vec<Seq> = (1..)
        .map(Seq::new)
        .zip(&events)
        .filter(|(_, body)| matches!(body, EventBody::OwnTurn(_)))
        .map(|(seq, _)| seq)
        .collect();
    let mut world = chat_world(SessionStatus::Idle, events);
    stopped_short(&mut world);
    let id = world.sessions.last().unwrap().id.clone();
    for seq in picked_up {
        world.idle_before(&id, seq, APART_MS);
    }
    world
}

/// A turn that left a review with a subagent in the background, then a turn the agent started by itself as it
/// finished, which the account's usage limit refused before it did anything.
fn refused_picking_up() -> Vec<EventBody> {
    let reply = "The review is going. I'll pick up when it reports.";
    let mut events = first_turn(
        user_prompt("Have an agent review the depot's design."),
        [text("msg-1", "I'll have an agent review the depot in the background.")],
    );
    events.extend(review("msg-1", "toolu-1"));
    events.extend([text("msg-2", reply), replied(reply, 9, 0.04), subagent_ended("toolu-1", FOUND, 31_400)]);
    picks_up_refused(&mut events, ended(TaskKind::Subagent, "toolu-1", REVIEW, TaskOutcome::Finished));
    events
}

/// A turn that left a review and the tests in the background, and was refused partway through, then a turn the agent
/// started by itself as each ended, both refused the same way.
fn refused_partway_then_twice() -> Vec<EventBody> {
    let mut events = first_turn(
        user_prompt("Have an agent review the depot's design, and run the tests while you look at what #7 changes."),
        [text("msg-1", "I'll start a review and the tests in the background, then look at what #7 changes.")],
    );
    events.extend(review("msg-1", "toolu-1"));
    events.extend([
        in_background("msg-1", "toolu-2", "cargo test --workspace", "Run the tests"),
        printed("toolu-2", "Command running in background."),
        command("msg-2", "toolu-3", "gh pr diff 7 --name-only", Some("List what #7 changes")),
        printed("toolu-3", "crates/depot/src/signals.rs\ncrates/depot/src/routes.rs"),
    ]);
    refuse(&mut events);
    events.push(subagent_ended("toolu-1", FOUND, 31_400));
    picks_up_refused(&mut events, ended(TaskKind::Subagent, "toolu-1", REVIEW, TaskOutcome::Finished));
    picks_up_refused(&mut events, ended(TaskKind::Command, "toolu-2", "Run the tests", TaskOutcome::Failed));
    events
}

/// A turn that left five tasks in the background, then a prompt the account's usage limit refused, and a turn the
/// agent started by itself as each task ended, all refused the same way, so the card folds the first two.
fn refused_six_times() -> Vec<EventBody> {
    let commands = [
        ("toolu-1", "cargo build -p desktop", "Build the desktop app"),
        ("toolu-2", "tools/check-links.sh docs/guide", "Check the guide's links"),
        ("toolu-3", "cargo test --workspace", "Run the full test suite"),
    ];
    let reply = "They're all going. I'll pick up as each one reports.";
    let mut events = first_turn(
        user_prompt("Build the app, check the guide's links, run the tests and review the depot, then queue #7."),
        [text("msg-1", "I'll start each of them in the background.")],
    );
    for (call, command, what_for) in commands {
        events
            .extend([in_background("msg-1", call, command, what_for), printed(call, "Command running in background.")]);
    }
    events.extend(review("msg-1", "toolu-4"));
    events.extend(wait("msg-1", "toolu-5"));
    events.extend([text("msg-2", reply), replied(reply, 41, 0.12)]);
    start_turn(&mut events, user_prompt("How are they going?"));
    refuse(&mut events);
    picks_up_refused(&mut events, ended(TaskKind::Command, "toolu-1", "Build the desktop app", TaskOutcome::Finished));
    picks_up_refused(
        &mut events,
        ended(TaskKind::Command, "toolu-2", "Check the guide's links", TaskOutcome::Finished),
    );
    events.push(subagent_ended("toolu-4", FOUND, 31_400));
    picks_up_refused(&mut events, ended(TaskKind::Subagent, "toolu-4", REVIEW, TaskOutcome::Finished));
    picks_up_refused(&mut events, waited("toolu-5"));
    picks_up_refused(&mut events, ended(TaskKind::Command, "toolu-3", "Run the full test suite", TaskOutcome::Failed));
    events
}

/// The call in message `message` that has a subagent review the depot in the background, and its start.
fn review(message: &str, call: &str) -> [EventBody; 3] {
    [
        subagent(message, call, "general-purpose", REVIEW, REVIEW_PROMPT, Runs::Background),
        subagent_started(call, "general-purpose", REVIEW, REVIEW_PROMPT),
        launched(call),
    ]
}

/// The call in message `message` that waits on #7's checks in the background.
fn wait(message: &str, call: &str) -> [EventBody; 2] {
    let asked = json!({ "pull_requests": [7], "until": "required_checks" });
    [
        mcp(message, call, "roundhouse", "wait_for_pull_requests", asked),
        mcp_said(call, "The task w1 was moved to the background.", false),
    ]
}

/// How the task the call `call` started ended, as the turn the agent started by itself took it in.
fn ended(kind: TaskKind, call: &str, description: &str, outcome: TaskOutcome) -> EndedTask {
    EndedTask { task: task(kind, call, description), outcome, wait: None }
}

/// How the wait the call `call` started ended, with #7's checks passing, as the hub records it.
fn waited(call: &str) -> EndedTask {
    let mut wait = protocol::fixtures::pull_request_wait(call);
    wait.ended = Some(WaitEnded { at_ms: 2, how: WaitEnd::ChecksPassed });
    let task = task(TaskKind::Wait, call, "roundhouse/wait_for_pull_requests");
    EndedTask { task, outcome: TaskOutcome::Finished, wait: Some(EndedWait::from(&wait)) }
}

/// A turn the agent started by itself to take in how `task` ended, refused before it did anything.
fn picks_up_refused(events: &mut Vec<EventBody>, task: EndedTask) {
    events.push(EventBody::OwnTurn(OwnTurn::TaskEnded { ended: vec![task] }));
    refuse(events);
}

/// A request the account's usage limit refused, which ends its turn, saying it repeats the newest failed request
/// before it, as the hub does when nothing between them ended the row, which nothing here does.
fn refuse(events: &mut Vec<EventBody>) {
    let before = events.iter().enumerate().rev().find_map(|(ix, body)| {
        let EventBody::RequestFailed { again, .. } = body else { return None };
        Some((ix, again.map_or(1, |again| again.times)))
    });
    let again =
        before.map(|(ix, times)| Again { previous: Seq::new(u64::try_from(ix).unwrap() + 1), times: times + 1 });
    events.extend(refused(NOW_MS + 2 * 60 * 60 * 1_000, again));
}
