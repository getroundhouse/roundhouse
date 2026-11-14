use serde_json::{Value, json};

use crate::conformance::lines::Lines;

pub(super) struct JsonRpc;

impl Lines for JsonRpc {
    fn kind(&self, line: &Value) -> String {
        line["method"].as_str().map_or_else(|| "reply".into(), str::to_owned)
    }
    fn fresh_ids(&self, _line: &Value) -> &'static [&'static [&'static str]] {
        &[]
    }
    fn by_itself(&self, _line: &Value) -> bool {
        false
    }
    fn answers<'a>(&self, line: &'a Value) -> Option<&'a str> {
        line.get("id").and_then(Value::as_str).filter(|_| line.get("method").is_none())
    }
    fn answer(&self, captured: Option<&Value>, request: &Value) -> Value {
        let mut answer = captured.cloned().unwrap_or_else(|| json!({"error":{"code":-32601,"message":"not captured"}}));
        answer["id"] = request["id"].clone();
        answer
    }
}
