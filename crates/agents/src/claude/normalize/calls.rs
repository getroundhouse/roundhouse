//! The agent's tool calls that are open: made, and not yet answered.

use std::collections::HashMap;

use protocol::model::ToolUseId;
use serde_json::Value;

use super::{id_of, str_of, string_of};

/// Every call that hasn't had its result, by the call's ID. A result closes its call whatever it says, so one that
/// fails or is refused doesn't stay open, and what a later line says of a call, such as the task it started, can be
/// read against the tool the call was for.
#[derive(Debug, Default)]
pub(super) struct OpenCalls(HashMap<ToolUseId, Call>);

#[derive(Debug)]
struct Call {
    tool: String,
    /// A subagent made it, so it can outlive the agent's own turn.
    by_a_subagent: bool,
}

impl OpenCalls {
    /// Opens the calls this line makes, and closes those it answers. Returns the tool of each call it answers, by the
    /// call, since once closed nothing else says what the call was for.
    pub(super) fn note(&mut self, value: &Value) -> Vec<(ToolUseId, String)> {
        let mut answered = Vec::new();
        let by_a_subagent = str_of(value, "parent_tool_use_id").is_some();
        let blocks = value.get("message").and_then(|message| message.get("content")).and_then(Value::as_array);
        for block in blocks.into_iter().flatten() {
            match str_of(block, "type") {
                Some("tool_use" | "server_tool_use") => {
                    if let (Some(id), Some(tool)) = (id_of(block, "id"), string_of(block, "name")) {
                        self.0.insert(id, Call { tool, by_a_subagent });
                    }
                }
                // A tool the API runs itself answers with a kind of result of its own, such as the advisor's.
                Some(kind) if kind.ends_with("tool_result") => {
                    if let Some(id) = id_of::<ToolUseId>(block, "tool_use_id")
                        && let Some(call) = self.0.remove(&id)
                    {
                        answered.push((id, call.tool));
                    }
                }
                _ => {}
            }
        }
        answered
    }

    /// The tool an open call is for. `None` once it's answered, and for a call that was never made.
    pub(super) fn tool(&self, call: &ToolUseId) -> Option<&str> {
        self.0.get(call).map(|call| call.tool.as_str())
    }

    /// The agent's own calls are over when its turn is, answered or not, as when the turn is cancelled. A subagent's
    /// go on, since it can work past the turn that started it.
    pub(super) fn turn_ended(&mut self) {
        self.0.retain(|_, call| call.by_a_subagent);
    }
}

#[cfg(test)]
mod tests {
    use protocol::model::ToolUseId;
    use serde_json::{Value, json};

    use super::OpenCalls;

    fn id(call: &str) -> ToolUseId {
        call.into()
    }

    fn call(id: &str, tool: &str, parent: Option<&str>) -> Value {
        json!({"type":"assistant","parent_tool_use_id":parent,
            "message":{"content":[{"type":"tool_use","id":id,"name":tool,"input":{}}]}})
    }

    fn result(id: &str, is_error: bool) -> Value {
        json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":id,"is_error":is_error}]}})
    }

    #[test]
    fn a_call_is_open_from_when_its_made_until_its_result_whatever_the_result_says() {
        let mut calls = OpenCalls::default();
        assert_eq!(calls.tool(&id("c1")), None, "not made yet");

        calls.note(&call("c1", "Monitor", None));
        calls.note(&call("c2", "Bash", None));
        assert_eq!((calls.tool(&id("c1")), calls.tool(&id("c2"))), (Some("Monitor"), Some("Bash")));

        calls.note(&result("c1", true));
        assert_eq!(
            (calls.tool(&id("c1")), calls.tool(&id("c2"))),
            (None, Some("Bash")),
            "a refused call is closed too"
        );
        calls.note(&result("c2", false));
        assert_eq!(calls.tool(&id("c2")), None);
    }

    #[test]
    fn a_tool_the_api_runs_is_answered_by_a_result_of_its_own_kind() {
        let mut calls = OpenCalls::default();
        let asked = json!({"type":"assistant","message":{"content":[
            {"type":"server_tool_use","id":"s1","name":"advisor","input":{}}]}});
        let answered = json!({"type":"assistant","message":{"content":[
            {"type":"advisor_tool_result","tool_use_id":"s1","content":{}}]}});

        calls.note(&asked);
        assert_eq!(calls.tool(&id("s1")), Some("advisor"));
        calls.note(&answered);
        assert_eq!(calls.tool(&id("s1")), None);
    }

    #[test]
    fn a_turns_end_closes_the_agents_own_calls_and_leaves_a_subagents() {
        let mut calls = OpenCalls::default();
        calls.note(&call("own", "Bash", None));
        calls.note(&call("theirs", "Monitor", Some("agent-call")));

        calls.turn_ended();

        assert_eq!((calls.tool(&id("own")), calls.tool(&id("theirs"))), (None, Some("Monitor")));
    }
}
