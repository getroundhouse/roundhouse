//! Chats that show each kind of tool call, closed and opened, for the screens about them. Made up, about a small
//! change to a signalling crate, so they read like real work.

#![cfg(test)]

use protocol::event::EventBody;
use serde_json::json;

use super::events::{
    advice_failed, advised, advised_encrypted, replied, taken, text, tool_failed, tool_result, tool_use, user_prompt,
};

/// What the chats about the signalling crate were asked.
pub(crate) const ASK: &str = "Make signals say whether a train may pass, and add a test for it.";

const TEST_OUTPUT: &str = "   Compiling signals v0.1.0 (/workspaces/aspen/crates/signals)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.91s
     Running unittests src/lib.rs (target/debug/deps/signals-3f2a9c1e)

running 2 tests
test signal_test::stop_may_not_pass ... ok
test signal_test::clear_and_caution_may_pass ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out";

const FORMAT_FAILURE: &str = "\
Exit code 1
Diff in /workspaces/aspen/crates/signals/src/signal_test.rs:4:
 fn stop_may_not_pass() {
-    assert!(!Aspect::Stop.may_pass()) ;
+    assert!(!Aspect::Stop.may_pass());
 }";

/// A command too long for its line: two searches, one after the other, as an agent writes them.
pub(crate) const SEARCH: &str = "\
    gh search issues --repo example/signals \"stuck at stop\" --sort created --order desc --limit 5 \
    --json number,title,createdAt,state --jq '.[] | \"\\(.number) \\(.state) \\(.createdAt) \\(.title)\"' \
    2>&1; gh search issues --repo example/signals \"may_pass\" --sort created --order desc --limit 5 \
    --json number,title,createdAt,state --jq '.[] | \"\\(.number) \\(.state) \\(.createdAt) \\(.title)\"' 2>&1";

/// What the agent said the searches are for.
pub(crate) const SEARCH_LINE: &str = "Search the signal issues for trains stuck at stop";

/// A command that prints nothing when it works.
pub(crate) const FORMAT_LINE: &str = "Format the workspace";

/// What the searches printed, each line too long for the block.
const SEARCH_OUTPUT: &str = "\
271 open 2026-11-11T15:09:45Z Trains stay stuck at a stop signal after the depot's check moved to may_pass, since 14:00 UTC
268 open 2026-11-11T14:46:52Z signal_test: stop_may_not_pass fails on the depot branch, so the build has been red since 13:48
204 closed 2025-08-03T13:48:53Z Stop signals ignore caution in the depot";

/// Commands: one that failed, one that printed nothing, one with no description, and two too long for their line,
/// one opened to its output.
pub(crate) fn commands() -> Vec<EventBody> {
    let reply = "The tests pass, and the formatting passes once `cargo fmt` has run.";
    vec![
        user_prompt(ASK),
        taken(1),
        text("msg-1", "I'll run the tests and check the formatting before changing anything."),
        tool_use(
            "msg-2",
            "toolu-1",
            "Bash",
            json!({ "command": "cargo test -p signals may_pass", "description": "Run the signal tests" }),
        ),
        tool_result("toolu-1", TEST_OUTPUT),
        tool_use(
            "msg-3",
            "toolu-2",
            "Bash",
            json!({ "command": "cargo fmt --all --check", "description": "Check formatting" }),
        ),
        tool_failed("toolu-2", FORMAT_FAILURE),
        tool_use("msg-4", "toolu-3", "Bash", json!({ "command": "cargo fmt --all", "description": FORMAT_LINE })),
        tool_result("toolu-3", ""),
        tool_use("msg-5", "toolu-4", "Bash", json!({ "command": "git status --short" })),
        tool_result("toolu-4", " M crates/signals/src/signal_test.rs"),
        tool_use("msg-6", "toolu-5", "Bash", json!({ "command": SEARCH, "description": SEARCH_LINE })),
        tool_result("toolu-5", SEARCH_OUTPUT),
        tool_use(
            "msg-7",
            "toolu-6",
            "Bash",
            json!({
                "command": "cargo fmt --all && cargo clippy --workspace --all-targets --locked -- -D warnings",
                "description": "Format and lint the workspace",
            }),
        ),
        tool_result("toolu-6", "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.20s"),
        text("msg-8", reply),
        replied(reply, 31, 0.12),
    ]
}

/// Tools without a drawing of their own: one from an MCP server opened, one that failed, and a built-in one.
pub(crate) fn any_tool() -> Vec<EventBody> {
    let reply = "Filed as #271.";
    vec![
        user_prompt("File the idea about saying why a train stopped."),
        taken(1),
        text("msg-1", "I'll look for an existing issue first, then file one."),
        tool_use(
            "msg-2",
            "toolu-1",
            "mcp__sentry__search_issues",
            json!({ "query": "signal stopped without a reason", "project": "signals" }),
        ),
        tool_failed("toolu-1", "<tool_use_error>Not authorized for the project signals</tool_use_error>"),
        tool_use(
            "msg-3",
            "toolu-2",
            "ToolSearch",
            json!({ "query": "select:mcp__github__create_issue", "max_results": 1 }),
        ),
        tool_result("toolu-2", "mcp__github__create_issue"),
        tool_use(
            "msg-4",
            "toolu-3",
            "mcp__github__create_issue",
            json!({
                "title": "Signals should say why a train stopped",
                "body": "A train that stops at a signal logs \"stop\", but not which signal or why.",
                "labels": ["kind/feature"],
            }),
        ),
        tool_result("toolu-3", "Created issue #271: https://github.com/example/signals/issues/271"),
        text("msg-5", reply),
        replied(reply, 14, 0.05),
    ]
}

/// Asking the advisor: a reply encrypted for the model alone, a call the agent went on without and made again, whose
/// reply can be read, and a call that failed.
pub(crate) fn advisor() -> Vec<EventBody> {
    let reply = "`Aspect::may_pass` now says whether a train may pass, with a test for each aspect.";
    let advice = "Every aspect is covered now. Run `cargo test -p signals` before **finishing**.";
    vec![
        user_prompt(ASK),
        taken(1),
        text("msg-1", "Before I change `Aspect`, I'll ask the advisor whether the plan holds up."),
        tool_use("msg-2", "srvtoolu-1", "advisor", json!({})),
        advised_encrypted("msg-2", "srvtoolu-1"),
        text("msg-3", "It suggests testing caution as well as stop, so I'll cover every aspect."),
        tool_use("msg-3", "srvtoolu-0", "advisor", json!({})),
        EventBody::ToolCallDropped { tool_use_id: "srvtoolu-0".into() },
        tool_use("msg-4", "srvtoolu-2", "advisor", json!({})),
        advised("msg-4", "srvtoolu-2", advice),
        tool_use("msg-5", "srvtoolu-3", "advisor", json!({})),
        advice_failed("msg-5", "srvtoolu-3", "overloaded"),
        text("msg-6", reply),
        replied(reply, 48, 0.31),
    ]
}

/// A web search, a fetch of a page it found, and a fetch a site refused.
pub(crate) fn web() -> Vec<EventBody> {
    let reply = "A flashing yellow means the train may go on, but should be ready to stop at the next signal.";
    let query = "flashing yellow railway signal meaning";
    let links = json!([
        { "title": "Railway signal", "url": "https://en.wikipedia.org/wiki/Railway_signal" },
        { "title": "Signal aspects and what they tell a driver", "url": "https://www.example.org/signals/aspects" },
        { "title": "Reading the line: a guide to signals", "url": "https://signals.example.com/guide" },
    ]);
    let results = format!(
        "Web search results for query: \"{query}\"\n\nLinks: {links}\n\nA flashing yellow usually means the train may \
         go on, but the next signal is at caution, so it should be ready to slow down. Railways differ, and some use \
         it to warn of a diverging route.\n\n\nREMINDER: You MUST include the sources above in your response to the \
         user using markdown hyperlinks."
    );
    let answer = "A **flashing yellow** is a preliminary caution: the train may pass, and the next signal shows \
                  caution.\n\n- Some railways use it to warn of a diverging route ahead.\n- Others use it for a \
                  medium speed approach.";
    vec![
        user_prompt("What does a flashing yellow signal mean on a railway?"),
        taken(1),
        tool_use("msg-1", "toolu-1", "WebSearch", json!({ "query": query })),
        tool_result("toolu-1", &results),
        tool_use(
            "msg-2",
            "toolu-2",
            "WebFetch",
            json!({
                "url": "https://en.wikipedia.org/wiki/Railway_signal",
                "prompt": "What does a flashing yellow aspect mean, and does it differ between railways?",
            }),
        ),
        tool_result("toolu-2", answer),
        tool_use(
            "msg-3",
            "toolu-3",
            "WebFetch",
            json!({ "url": "https://signals.example.com/guide", "prompt": "What does a flashing yellow mean here?" }),
        ),
        tool_failed("toolu-3", "The server returned HTTP 403 Forbidden."),
        text("msg-4", reply),
        replied(reply, 21, 0.08),
    ]
}
