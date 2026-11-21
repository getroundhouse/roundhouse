//! A chat that hands work to subagents: one that searched while the agent read, and one that checks the change. The
//! same made-up signalling crate as the other chats.

use client_core::fake_hub::events::{
    command, first_turn, launched, on, printed, read, read_back, replied, subagent, subagent_context, subagent_ended,
    subagent_started, subagents_listed, text, user_prompt, within,
};
use protocol::capability::calls::call::Runs;
use protocol::event::EventBody;

/// What the first subagent is for, which its call says, and which the screen that shows its work is titled.
pub const SEARCH: &str = "Find checks for a stop signal";

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
pub fn subagents(working: bool) -> Vec<EventBody> {
    let mut events = first_turn(
        user_prompt("Find every place that treats a stop signal specially, and make them use `may_pass`."),
        [
            text("msg-1", "I'll have an agent find them while I read the signal."),
            subagent("msg-2", "toolu-1", "Explore", SEARCH, FIND_PROMPT, Runs::Background),
            subagent_started("toolu-1", "Explore", SEARCH, FIND_PROMPT),
            subagents_listed(&[("toolu-1", SEARCH)]),
            launched("toolu-1"),
        ],
    );
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
