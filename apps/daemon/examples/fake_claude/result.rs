//! The `result` line that ends a turn that went well, as the real CLI writes it.

use serde_json::{Value, json};

/// The same for every turn, with what the turn's own requests took, so a test knows what a turn shows, and how much
/// the model holds.
pub(crate) fn success(session_id: &str) -> Value {
    let model = super::current_model();
    json!({"type":"result","subtype":"success","is_error":false,"terminal_reason":"completed","result":"done",
        "duration_ms":1,"num_turns":1,"total_cost_usd":0.0,"session_id":session_id,
        "usage":{"input_tokens":6,"output_tokens":550,"cache_read_input_tokens":1_142_851,
            "cache_creation_input_tokens":1_615},
        "modelUsage":{model:{"contextWindow":super::account::MODEL_TOKENS}}})
}
