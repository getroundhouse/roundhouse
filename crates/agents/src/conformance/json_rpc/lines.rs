use serde_json::{Value, json};

use crate::conformance::lines::Lines;
use crate::jsonrpc::Dialect;

/// The lines of a JSON-RPC conversation in `dialect`, where a reply names the request it answers by its ID.
pub(super) struct JsonRpc {
    dialect: Dialect,
}

static BARE: JsonRpc = JsonRpc { dialect: Dialect::Bare };
static JSON_RPC_2: JsonRpc = JsonRpc { dialect: Dialect::JsonRpc2 };

impl JsonRpc {
    pub(super) fn of(dialect: Dialect) -> &'static JsonRpc {
        match dialect {
            Dialect::Bare => &BARE,
            Dialect::JsonRpc2 => &JSON_RPC_2,
        }
    }
}

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
        let mut answer = captured
            .cloned()
            .unwrap_or_else(|| self.dialect.framed(json!({ "error": { "code": -32601, "message": "not captured" } })));
        answer["id"] = request["id"].clone();
        answer
    }
}
