//! Chats that hand work to subagents, for the screen about those calls. The same made-up signalling crate as
//! `tool_chats.rs`.

#![cfg(test)]

use protocol::event::EventBody;
use serde_json::json;

use super::events::{finished_turn, taken, text, tool_result, tool_use, user_prompt, within};

/// What Claude sends back as soon as a subagent starts in the background.
const LAUNCHED: &str = "Async agent launched successfully.";

const FIND_PROMPT: &str = "In /workspaces/aspen, find every place that checks whether a signal shows Stop, such as \
                           `== Aspect::Stop` or a match arm. List each with its file and line, and say what it does \
                           there. Read-only: don't change anything.";

const REPORT: &str = "Two places check for a stop signal outside the signal itself:

- `crates/signals/src/train.rs:53` stops the train when the signal ahead shows Stop.
- `crates/signals/src/depot.rs:19` won't send a train onto a line whose signal shows Stop.

Both compare with `Aspect::Stop` directly, so either could use `may_pass`.";

/// A subagent that searched while the agent read, its work interleaved with the agent's own, and a second one
/// that checked the change.
pub(crate) fn subagents() -> Vec<EventBody> {
    let mut events = vec![
        user_prompt("Find every place that treats a stop signal specially, and make them use `may_pass`."),
        taken(1),
        text("msg-1", "I'll have an agent find them while I read the signal."),
        tool_use(
            "msg-2",
            "toolu-1",
            "Agent",
            json!({ "description": "Find checks for a stop signal", "subagent_type": "Explore", "prompt": FIND_PROMPT }),
        ),
        tool_result("toolu-1", LAUNCHED),
    ];
    events.extend(finding("toolu-1"));
    events.extend([
        text("msg-4", "It found two. I'll change both to use `may_pass`, and have them checked."),
        tool_use(
            "msg-5",
            "toolu-3",
            "Agent",
            json!({
                "description": "Review the change",
                "subagent_type": "general-purpose",
                "prompt": "Check that train.rs and depot.rs now use `may_pass`, and that the signal tests pass.",
            }),
        ),
        tool_result("toolu-3", LAUNCHED),
        within(
            "toolu-3",
            tool_use(
                "sub-6",
                "toolu-s4",
                "Bash",
                json!({ "command": "cargo test -p signals", "description": "Run the signal tests" }),
            ),
        ),
        within("toolu-3", tool_result("toolu-s4", "test result: ok. 5 passed; 0 failed")),
        within("toolu-3", text("sub-7", "Both now use `may_pass`, and the signal tests pass.")),
        text("msg-6", "Both checks use `may_pass` now, and the tests pass."),
        finished_turn(94, 0.41),
    ]);
    events
}

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
