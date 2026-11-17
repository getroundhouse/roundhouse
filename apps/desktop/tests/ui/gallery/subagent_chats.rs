//! Chats that hand work to subagents, and the screens about those calls and the subagent tab, which the gallery takes
//! after `screens.rs`'s and `tool_screens.rs`'s. The same made-up signalling crate as `tool_chats.rs`.

#![cfg(test)]

use protocol::capability::calls::call::{MessageSubagent, Read, Runs, StartSubagent, StopSubagent};
use protocol::capability::calls::output::Ended;
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::EventBody;
use protocol::fixtures::{command_output, typed_command};
use protocol::model::{ModelName, Seq, SessionStatus};

use super::frame::{CHAT, TABS_AND_CHAT};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::worlds::chat_world;
use client_core::fake_hub::events::{
    command, launched, on, printed, read, read_back, replied, subagent, subagent_context, subagent_ended,
    subagent_started, subagents_listed, taken, text, typed_call, typed_result, user_prompt, within,
};

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

const FIND_PROMPT: &str = "In /workspaces/aspen, find every place that checks whether a signal shows Stop, such as \
                           `== Aspect::Stop` or a match arm. List each with its file and line, and say what it does \
                           there. Read-only: don't change anything.";

const REPORT: &str = "Two places check for a stop signal outside the signal itself:

- `crates/signals/src/train.rs:53` stops the train when the signal ahead shows Stop.
- `crates/signals/src/depot.rs:19` won't send a train onto a line whose signal shows Stop.

Both compare with `Aspect::Stop` directly, so either could use `may_pass`.";

/// The first subagent's search, on Haiku, with the agent's own read of the signal in the middle of it.
fn finding(agent: &str) -> Vec<EventBody> {
    let found = "crates/signals/src/signal.rs:16:            Self::Caution => Self::Stop,\n\
                 crates/signals/src/train.rs:53:        if signal == Aspect::Stop {\n\
                 crates/signals/src/depot.rs:19:    if aspect == Aspect::Stop { return None; }";
    let events = vec![
        within(agent, text("sub-1", "I'll search for comparisons with the stop aspect first.")),
        within(agent, command("sub-2", "toolu-s1", "rg -n 'Aspect::Stop' crates", Some("Find comparisons with Stop"))),
        read("msg-3", "toolu-2", "/workspaces/aspen/crates/signals/src/signal.rs"),
        read_back("toolu-2", 24, "impl Aspect {\n    /// The next aspect down the line, as a train passes."),
        within(agent, printed("toolu-s1", found)),
        within(agent, read("sub-3", "toolu-s2", "/workspaces/aspen/crates/signals/src/train.rs")),
        within(
            agent,
            read_back(
                "toolu-s2",
                52,
                "    fn approach(&mut self, signal: Aspect) {\n        if signal == Aspect::Stop {\n            self.brake();",
            ),
        ),
        within(agent, read("sub-4", "toolu-s3", "/workspaces/aspen/crates/signals/src/depot.rs")),
        within(
            agent,
            read_back(
                "toolu-s3",
                18,
                "pub fn release(&mut self, aspect: Aspect) -> Option<Train> {\n    if aspect == Aspect::Stop { return None; }",
            ),
        ),
        within(agent, text("sub-5", REPORT)),
    ];
    let haiku = |event: EventBody| if event.subagent().is_some() { on("claude-haiku-5-5", event) } else { event };
    events.into_iter().map(haiku).collect()
}

/// What the second subagent, started by the call `agent`, has done while the screens show it at work: it read both
/// files, then started the signal tests.
fn reviewing(agent: &str) -> Vec<EventBody> {
    vec![
        within(agent, text("sub-6", "I'll read both files, then run the signal tests.")),
        within(agent, read("sub-7", "toolu-s5", "/workspaces/aspen/crates/signals/src/train.rs")),
        within(agent, read_back("toolu-s5", 53, "        if signal.may_pass() {")),
        within(agent, read("sub-10", "toolu-s6", "/workspaces/aspen/crates/signals/src/depot.rs")),
        within(agent, read_back("toolu-s6", 19, "    if !aspect.may_pass() { return None; }")),
        within(agent, text("sub-11", "Both call `may_pass` now. Running the tests.")),
        within(agent, command("sub-8", "toolu-s4", "cargo test -p signals", Some("Run the signal tests"))),
    ]
}

const REVIEW_PROMPT: &str = "Check that train.rs and depot.rs now use `may_pass`, and that the signal tests pass.";

/// A subagent that searched while the agent read, its work interleaved with the agent's own, and a second one that
/// checks the change, with the agent's word on when each started and ended, and what was in the background, as the hub
/// records it. With `working`, the second is still at its tests, and the agent waits on it.
pub(crate) fn subagents(working: bool) -> Vec<EventBody> {
    let mut events = vec![
        user_prompt("Find every place that treats a stop signal specially, and make them use `may_pass`."),
        taken(Seq::new(1)),
        text("msg-1", "I'll have an agent find them while I read the signal."),
        subagent("msg-2", "toolu-1", "Explore", "Find checks for a stop signal", FIND_PROMPT, Runs::Background),
        subagent_started("toolu-1", "Explore", "Find checks for a stop signal", FIND_PROMPT),
        subagents_listed(&[("toolu-1", "Find checks for a stop signal")]),
        launched("toolu-1"),
    ];
    events.extend(finding("toolu-1"));
    events.extend([
        subagent_ended("toolu-1", REPORT, 18_240),
        subagents_listed(&[]),
        text("msg-4", "It found two. I'll change both to use `may_pass`, and have them checked."),
        subagent("msg-5", "toolu-3", "general-purpose", "Review the change", REVIEW_PROMPT, Runs::Background),
        subagent_started("toolu-3", "general-purpose", "Review the change", REVIEW_PROMPT),
        subagents_listed(&[("toolu-3", "Review the change")]),
        launched("toolu-3"),
    ]);
    events.extend(reviewing("toolu-3"));
    events.push(subagent_context("toolu-3", 24_610));
    if working {
        return events;
    }
    let reply = "Both checks use `may_pass` now, and the tests pass.";
    let checked = "Both now use `may_pass`, and the signal tests pass.";
    events.extend([
        within("toolu-3", printed("toolu-s4", "test result: ok. 5 passed; 0 failed")),
        within("toolu-3", text("sub-9", checked)),
        subagent_ended("toolu-3", checked, 26_130),
        subagents_listed(&[]),
        text("msg-6", reply),
        replied(reply, 94, 0.41),
    ]);
    events
}

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
    vec![
        user_prompt("Have someone check that train.rs and depot.rs use `may_pass` now."),
        taken(Seq::new(1)),
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
    ]
}

/// A subagent an agent that types its calls started, at work, as Codex's adapter records it: it's run the tests, and
/// is reading a file, which hasn't come back yet, so its call shows what it's done lately from its record.
fn typed_subagent_working() -> Vec<EventBody> {
    let tested = command_output("test result: ok. 5 passed; 0 failed", Some(0), Ended::Exited);
    let read = Read { path: "/workspaces/aspen/crates/signals/src/depot.rs".into(), lines: None, pages: None };
    let sub = |event| on(LUNA, within("thread-1", event));
    vec![
        user_prompt("Have someone check that train.rs and depot.rs use `may_pass` now."),
        taken(Seq::new(1)),
        text("msg-1", "I'll have a reviewer check the change."),
        typed_call("msg-2", "spawn-1", Call::Subagent(codex_subagent(REVIEWER))),
        codex_started("spawn-1", "thread-1", REVIEWER),
        sub(text("sub-1", "I'll run the signal tests first.")),
        sub(typed_call("sub-2", "run-1", Call::Command(typed_command("cargo test -p signals")))),
        within("thread-1", typed_result("run-1", tested, false)),
        sub(text("sub-3", "They pass. Now the checks themselves.")),
        sub(typed_call("sub-4", "read-1", Call::Read(read))),
    ]
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
    vec![
        user_prompt("Have someone check that train.rs uses `may_pass` now, and that the docs say so."),
        taken(Seq::new(1)),
        text("msg-1", "I'll have one reviewer check the code, and another the docs."),
        typed_call("msg-2", "spawn-1", Call::Subagent(codex_subagent(REVIEWER))),
        typed_call("msg-3", "spawn-2", Call::Subagent(codex_subagent(DOCS))),
        codex_started("spawn-1", "thread-1", REVIEWER),
        codex_started("spawn-2", "thread-2", DOCS),
        text("msg-4", "The docs never name the check, so I'll stop that one, and have the reviewer look at depot.rs."),
        typed_call("msg-5", "stop-1", Call::StopSubagent(StopSubagent { subagent: "thread-2".into() })),
        typed_call("msg-6", "tell-1", Call::MessageSubagent(message)),
    ]
}
