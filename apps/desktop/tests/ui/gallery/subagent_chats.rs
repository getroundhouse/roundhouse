//! Chats that hand work to subagents, and the screens about those calls and the subagent tab, which the gallery takes
//! after `screens.rs`'s and `tool_screens.rs`'s. The same made-up signalling crate as `tool_chats.rs`.

#![cfg(test)]

use protocol::event::EventBody;
use protocol::model::SessionStatus;
use serde_json::json;

use super::events::{
    replied, subagent_ended, subagent_started, taken, text, tool_result, tool_use, user_prompt, within,
};
use super::frame::{CHAT, TABS_AND_CHAT};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::worlds::chat_world;

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

/// What Claude sends back as soon as a subagent starts in the background.
const LAUNCHED: &str = "Async agent launched successfully.";

const FIND_PROMPT: &str = "In /workspaces/aspen, find every place that checks whether a signal shows Stop, such as \
                           `== Aspect::Stop` or a match arm. List each with its file and line, and say what it does \
                           there. Read-only: don't change anything.";

const REPORT: &str = "Two places check for a stop signal outside the signal itself:

- `crates/signals/src/train.rs:53` stops the train when the signal ahead shows Stop.
- `crates/signals/src/depot.rs:19` won't send a train onto a line whose signal shows Stop.

Both compare with `Aspect::Stop` directly, so either could use `may_pass`.";

/// The first subagent's search, with the agent's own read of the signal in the middle of it.
fn finding(agent: &str) -> Vec<EventBody> {
    let found = "crates/signals/src/signal.rs:16:            Self::Caution => Self::Stop,\n\
                 crates/signals/src/train.rs:53:        if signal == Aspect::Stop {\n\
                 crates/signals/src/depot.rs:19:    if aspect == Aspect::Stop { return None; }";
    vec![
        within(agent, text("sub-1", "I'll search for comparisons with the stop aspect first.")),
        within(
            agent,
            tool_use(
                "sub-2",
                "toolu-s1",
                "Bash",
                json!({ "command": "rg -n 'Aspect::Stop' crates", "description": "Find comparisons with Stop" }),
            ),
        ),
        tool_use("msg-3", "toolu-2", "Read", json!({ "file_path": "/workspaces/aspen/crates/signals/src/signal.rs" })),
        tool_result(
            "toolu-2",
            "    24\timpl Aspect {\n    25\t    /// The next aspect down the line, as a train passes.",
        ),
        within(agent, tool_result("toolu-s1", found)),
        within(
            agent,
            tool_use(
                "sub-3",
                "toolu-s2",
                "Read",
                json!({ "file_path": "/workspaces/aspen/crates/signals/src/train.rs" }),
            ),
        ),
        within(
            agent,
            tool_result(
                "toolu-s2",
                "    52\t    fn approach(&mut self, signal: Aspect) {\n    53\t        if signal == Aspect::Stop {\n    54\t            self.brake();",
            ),
        ),
        within(
            agent,
            tool_use(
                "sub-4",
                "toolu-s3",
                "Read",
                json!({ "file_path": "/workspaces/aspen/crates/signals/src/depot.rs" }),
            ),
        ),
        within(
            agent,
            tool_result(
                "toolu-s3",
                "    18\tpub fn release(&mut self, aspect: Aspect) -> Option<Train> {\n    19\t    if aspect == Aspect::Stop { return None; }",
            ),
        ),
        within(agent, text("sub-5", REPORT)),
    ]
}

/// What the second subagent, started by the call `agent`, has done while the screens show it at work: it read both
/// files, then started the signal tests.
fn reviewing(agent: &str) -> Vec<EventBody> {
    vec![
        within(agent, text("sub-6", "I'll read both files, then run the signal tests.")),
        within(
            agent,
            tool_use(
                "sub-7",
                "toolu-s5",
                "Read",
                json!({ "file_path": "/workspaces/aspen/crates/signals/src/train.rs" }),
            ),
        ),
        within(agent, tool_result("toolu-s5", "    53\t        if signal.may_pass() {")),
        within(
            agent,
            tool_use(
                "sub-10",
                "toolu-s6",
                "Read",
                json!({ "file_path": "/workspaces/aspen/crates/signals/src/depot.rs" }),
            ),
        ),
        within(agent, tool_result("toolu-s6", "    19\t    if !aspect.may_pass() { return None; }")),
        within(agent, text("sub-11", "Both call `may_pass` now. Running the tests.")),
        within(
            agent,
            tool_use(
                "sub-8",
                "toolu-s4",
                "Bash",
                json!({ "command": "cargo test -p signals", "description": "Run the signal tests" }),
            ),
        ),
    ]
}

const REVIEW_PROMPT: &str = "Check that train.rs and depot.rs now use `may_pass`, and that the signal tests pass.";

/// A subagent that searched while the agent read, its work interleaved with the agent's own, and a second one that
/// checks the change, with the agent's word on when each started and ended, as the hub records it. With `working`, the
/// second is still at its tests, and the agent waits on it.
pub(crate) fn subagents(working: bool) -> Vec<EventBody> {
    let find =
        json!({ "description": "Find checks for a stop signal", "subagent_type": "Explore", "prompt": FIND_PROMPT });
    let review =
        json!({ "description": "Review the change", "subagent_type": "general-purpose", "prompt": REVIEW_PROMPT });
    let mut events = vec![
        user_prompt("Find every place that treats a stop signal specially, and make them use `may_pass`."),
        taken(1),
        text("msg-1", "I'll have an agent find them while I read the signal."),
        tool_use("msg-2", "toolu-1", "Agent", find),
        subagent_started("toolu-1", "Explore", "Find checks for a stop signal", FIND_PROMPT),
        tool_result("toolu-1", LAUNCHED),
    ];
    events.extend(finding("toolu-1"));
    events.extend([
        subagent_ended("toolu-1", REPORT),
        text("msg-4", "It found two. I'll change both to use `may_pass`, and have them checked."),
        tool_use("msg-5", "toolu-3", "Agent", review),
        subagent_started("toolu-3", "general-purpose", "Review the change", REVIEW_PROMPT),
        tool_result("toolu-3", LAUNCHED),
    ]);
    events.extend(reviewing("toolu-3"));
    if working {
        return events;
    }
    let reply = "Both checks use `may_pass` now, and the tests pass.";
    let checked = "Both now use `may_pass`, and the signal tests pass.";
    events.extend([
        within("toolu-3", tool_result("toolu-s4", "test result: ok. 5 passed; 0 failed")),
        within("toolu-3", text("sub-9", checked)),
        subagent_ended("toolu-3", checked),
        text("msg-6", reply),
        replied(reply, 94, 0.41),
    ]);
    events
}
