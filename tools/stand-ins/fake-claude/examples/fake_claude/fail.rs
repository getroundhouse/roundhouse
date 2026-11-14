//! `fail:ERROR`: the agent's next request to its model failing, as the real CLI says when the model's API refuses it. It
//! writes a message of its own in the model's place, saying why, then ends the turn for the API's error. ERROR is the
//! CLI's word for the kind of failure, such as `rate_limit`, and a limit says when it resets.

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use crate::emit;
use crate::turn::{Turn, message_id};

/// What the CLI says, made up, with an address, as the real one's has.
pub(crate) const SAID: &str = "You've reached your usage limit. Manage it at https://example.com/usage, to continue.";

/// When a limit resets, in seconds, as the CLI says it.
pub(crate) const RESETS_AT: i64 = 1_700_000_000;

/// Whether the turn going failed, so it ends for that.
static FAILED: AtomicBool = AtomicBool::new(false);

/// Says why the request failed, and returns true, since the turn goes no further.
pub(crate) fn request(turn: &Turn, error: &str) -> bool {
    let limit =
        (error == "rate_limit").then(|| json!({"rate_limit_info": {"status": "rejected", "resetsAt": RESETS_AT}}));
    emit(
        turn.out,
        json!({"type": "assistant", "parent_tool_use_id": turn.parent, "session_id": turn.config.session_id,
            "message": {"id": message_id(), "model": "<synthetic>", "role": "assistant",
                "content": [{"type": "text", "text": SAID}]},
            "error": error, "is_api_error_message": true, "api_error_status": 429, "api_error_params": limit}),
    );
    FAILED.store(true, Ordering::SeqCst);
    true
}

/// The `result` line that ends a turn that failed, if the turn going did, which the next turn starts without.
pub(crate) fn ended(session_id: &str) -> Option<Value> {
    FAILED.swap(false, Ordering::SeqCst).then(|| {
        json!({"type": "result", "subtype": "success", "is_error": true, "terminal_reason": "api_error",
            "api_error_status": 429, "result": SAID, "duration_ms": 1, "num_turns": 1, "total_cost_usd": 0.0,
            "session_id": session_id})
    })
}
