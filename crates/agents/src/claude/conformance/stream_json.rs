//! What a capture's lines are in Claude's stream-json.

use serde_json::{Value, json};

use crate::conformance::lines::Lines;

/// The rules of Claude's stream-json, as a fixture and a replay need them.
pub(super) struct StreamJson;

/// The control requests the adapter makes by itself, after a turn ends or as limits change.
const ASKED_BY_ITSELF: &[&str] = &["get_context_usage", "get_usage"];

impl Lines for StreamJson {
    fn kind(&self, line: &Value) -> String {
        let subtype = line["request"]["subtype"].as_str().or_else(|| line["response"]["subtype"].as_str());
        let kind = line["type"].as_str().unwrap_or("a line that isn't an object");
        subtype.map_or_else(|| kind.to_string(), |subtype| format!("{kind}/{subtype}"))
    }

    /// A control request's ID, and a prompt's `uuid`. An answer to the CLI's own request keeps the CLI's ID, which
    /// a replay writes as it was.
    fn fresh_ids(&self, line: &Value) -> &'static [&'static [&'static str]] {
        match line["type"].as_str() {
            Some("control_request") => &[&["request_id"]],
            Some("user") => &[&["uuid"]],
            _ => &[],
        }
    }

    fn by_itself(&self, line: &Value) -> bool {
        line["type"] == "control_request"
            && line["request"]["subtype"].as_str().is_some_and(|subtype| ASKED_BY_ITSELF.contains(&subtype))
    }

    fn answers<'a>(&self, line: &'a Value) -> Option<&'a str> {
        line["response"]["request_id"].as_str().filter(|_| line["type"] == "control_response")
    }

    fn answer(&self, captured: Option<&Value>, request: &Value) -> Value {
        let mut answer = captured.cloned().unwrap_or_else(|| {
            json!({"type": "control_response",
                   "response": {"subtype": "error", "error": "the capture has no answer to this"}})
        });
        answer["response"]["request_id"] = request["request_id"].clone();
        answer
    }
}
