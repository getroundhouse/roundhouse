//! Chats that hand work to subagents, and the screens about those calls and the subagent tab, which the gallery takes
//! after `screens.rs`'s and `tool_screens.rs`'s. The same made-up signalling crate as `tool_chats.rs`.

#![cfg(test)]

use protocol::capability::calls::call::{MessageSubagent, Read, Runs, StartSubagent, StopSubagent};
use protocol::capability::calls::output::Ended;
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::EventBody;
use protocol::fixtures::{command_output, typed_command};
use protocol::model::{ModelName, SessionStatus};

use super::frame::{CHAT, TABS_AND_CHAT};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::worlds::chat_world;
use client_core::fake_hub::events::{
    first_turn, on, replied, subagent_ended, text, typed_call, typed_result, user_prompt, within,
};
use spec_screens::subagent_chats::subagents;

/// The screens about subagents.
pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "tool-subagent",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Running, subagents(true))),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open("Find checks for a stop signal"), Step::Open("Review the change")],
    },
    Screen {
        name: "tool-subagent-typed",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, typed_subagent())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open(REVIEWER)],
    },
    Screen {
        name: "tool-subagent-typed-working",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Running, typed_subagent_working())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open(REVIEWER)],
    },
    Screen {
        name: "tool-subagent-messaged",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Running, messaged_subagents())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open("Message")],
    },
    Screen {
        name: "subagent-tab",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, subagents(false))),
        frame: TABS_AND_CHAT,
        steps: &[
            Step::Select(FOCUS),
            Step::Open("Find checks for a stop signal"),
            Step::Click("open-subagent/toolu-1"),
            Step::MoveAway,
        ],
    },
    Screen {
        name: "subagent-tab-working",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Running, subagents(true))),
        frame: TABS_AND_CHAT,
        steps: &[
            Step::Select(FOCUS),
            Step::Open("Review the change"),
            Step::Click("open-subagent/toolu-3"),
            Step::MoveAway,
        ],
    },
];

/// The task path Codex names its subagent by, which is all it says of what the subagent is for.
const REVIEWER: &str = "/root/reviewer";
/// The model the subagent runs on, one of Codex's.
const LUNA: &str = "gpt-6-luna";

/// The task path of another subagent, which checks what the docs say.
const DOCS: &str = "/root/docs";

/// A call that hands work to a subagent for `task`, as Codex's adapter types it: with its persona and no prompt, since
/// Codex doesn't say one.
fn codex_subagent(task: &str) -> StartSubagent {
    StartSubagent {
        persona: Some("explorer".into()),
        description: Some(task.into()),
        prompt: String::new(),
        model: Some(ModelName::named(LUNA)),
        effort: None,
        runs: Runs::Background,
        resumes: None,
    }
}

/// That the subagent `call` started for `task` did, named by its thread, as Codex's adapter says it.
fn codex_started(call: &str, thread: &str, task: &str) -> EventBody {
    EventBody::SubagentStarted {
        call: call.into(),
        subagent: Some(thread.into()),
        kind: Some("explorer".into()),
        description: Some(task.into()),
        prompt: None,
        in_background: true,
    }
}

/// A subagent an agent that types its calls started, as Codex's adapter records one: its call, typed, named by its
/// thread, which ran the signal tests in its own conversation, and its report as its call's result.
pub(crate) fn typed_subagent() -> Vec<EventBody> {
    let start = codex_subagent(REVIEWER);
    let report = "Both checks use `may_pass` now, and the signal tests pass.";
    let tested = command_output("test result: ok. 5 passed; 0 failed", Some(0), Ended::Exited);
    let reply = "The reviewer says both checks use `may_pass`, and the tests pass.";
    first_turn(
        user_prompt("Have someone check that train.rs and depot.rs use `may_pass` now."),
        [
            text("msg-1", "I'll have a reviewer check the change."),
            typed_call("msg-2", "spawn-1", Call::Subagent(start)),
            codex_started("spawn-1", "thread-1", REVIEWER),
            on(
                LUNA,
                within("thread-1", typed_call("sub-1", "run-1", Call::Command(typed_command("cargo test -p signals")))),
            ),
            within("thread-1", typed_result("run-1", tested, false)),
            on(LUNA, within("thread-1", text("sub-2", report))),
            subagent_ended("thread-1", report, 9_840),
            typed_result("spawn-1", CallOutput::Report { report: report.into() }, false),
            text("msg-3", reply),
            replied(reply, 41, 0.12),
        ],
    )
}

/// A subagent an agent that types its calls started, at work, as Codex's adapter records it: it's run the tests, and
/// is reading a file, which hasn't come back yet, so its call shows what it's done lately from its record.
fn typed_subagent_working() -> Vec<EventBody> {
    let tested = command_output("test result: ok. 5 passed; 0 failed", Some(0), Ended::Exited);
    let read = Read { path: "/workspaces/aspen/crates/signals/src/depot.rs".into(), lines: None, pages: None };
    let sub = |event| on(LUNA, within("thread-1", event));
    first_turn(
        user_prompt("Have someone check that train.rs and depot.rs use `may_pass` now."),
        [
            text("msg-1", "I'll have a reviewer check the change."),
            typed_call("msg-2", "spawn-1", Call::Subagent(codex_subagent(REVIEWER))),
            codex_started("spawn-1", "thread-1", REVIEWER),
            sub(text("sub-1", "I'll run the signal tests first.")),
            sub(typed_call("sub-2", "run-1", Call::Command(typed_command("cargo test -p signals")))),
            within("thread-1", typed_result("run-1", tested, false)),
            sub(text("sub-3", "They pass. Now the checks themselves.")),
            sub(typed_call("sub-4", "read-1", Call::Read(read))),
        ],
    )
}

/// Two subagents an agent that types its calls started, as Codex's adapter records them, partway through the turn:
/// the agent stops one, and tells the other to look at another file, and Codex hasn't answered either yet.
fn messaged_subagents() -> Vec<EventBody> {
    let message = MessageSubagent {
        subagent: "thread-1".into(),
        message: "Look at depot.rs too. It checks the signal before it sends a train out, so it should use `may_pass` \
                  as train.rs does now."
            .into(),
    };
    first_turn(
        user_prompt("Have someone check that train.rs uses `may_pass` now, and that the docs say so."),
        [
            text("msg-1", "I'll have one reviewer check the code, and another the docs."),
            typed_call("msg-2", "spawn-1", Call::Subagent(codex_subagent(REVIEWER))),
            typed_call("msg-3", "spawn-2", Call::Subagent(codex_subagent(DOCS))),
            codex_started("spawn-1", "thread-1", REVIEWER),
            codex_started("spawn-2", "thread-2", DOCS),
            text(
                "msg-4",
                "The docs never name the check, so I'll stop that one, and have the reviewer look at depot.rs.",
            ),
            typed_call("msg-5", "stop-1", Call::StopSubagent(StopSubagent { subagent: "thread-2".into() })),
            typed_call("msg-6", "tell-1", Call::MessageSubagent(message)),
        ],
    )
}
