//! Chats that show each kind of tool call, closed and opened, for the screens about them. Made up, about a small
//! change to a signalling crate, so they read like real work.

#![cfg(test)]

use protocol::event::EventBody;
use serde_json::json;

use super::events::{finished_turn, text, tool_failed, tool_result, tool_use, user_prompt};

const ASK: &str = "Make signals say whether a train may pass, and add a test for it.";

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

/// Commands: one opened with its output, one that failed, one with no description, and one too long for its line.
pub(crate) fn commands() -> Vec<EventBody> {
    vec![
        user_prompt(ASK),
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
        tool_use("msg-4", "toolu-3", "Bash", json!({ "command": "git status --short" })),
        tool_result("toolu-3", " M crates/signals/src/signal_test.rs"),
        tool_use(
            "msg-5",
            "toolu-4",
            "Bash",
            json!({
                "command": "cargo fmt --all && cargo clippy --workspace --all-targets --locked -- -D warnings",
                "description": "Format and lint the workspace",
            }),
        ),
        tool_result("toolu-4", "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.20s"),
        text("msg-6", "The tests pass, and the formatting passes once `cargo fmt` has run."),
        finished_turn(31, 0.12),
    ]
}

/// Tools without a drawing of their own: one from an MCP server opened, one that failed, and a built-in one.
pub(crate) fn any_tool() -> Vec<EventBody> {
    vec![
        user_prompt("File the idea about saying why a train stopped."),
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
        text("msg-5", "Filed as #271."),
        finished_turn(14, 0.05),
    ]
}
