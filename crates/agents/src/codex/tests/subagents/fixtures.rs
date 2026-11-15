//! Codex's messages about subagents, for the tests of how its adapter records them.

use serde_json::{Value, json};

use super::super::script::item;

pub(super) fn activity(thread: &str, turn: &str, child: &str, call: &str, kind: &str, done: bool) -> Value {
    item(
        thread,
        turn,
        json!({"type":"subAgentActivity","id":call,"kind":kind,"agentThreadId":child,
        "agentPath":format!("/root/{child}"),"model":"gpt-6-luna","reasoningEffort":"low"}),
        done,
    )
}
