//! How the agent's requests meet the API's cache, as each message's start says.

use parking_lot::Mutex;
use serde_json::{Value, json};

use crate::emit;
use crate::turn::Turn;

/// How long the model takes to start answering, as each message's start says: long enough that a hub that didn't take
/// it off when the request was sent would be seen to.
pub(crate) const FIRST_TOKEN_MS: u64 = 60_000;

/// The next of the agent's own messages finds the cache expired, as `cold` says.
static COLD: Mutex<bool> = Mutex::new(false);

pub(crate) fn expire() {
    *COLD.lock() = true;
}

/// The start of the message `message_id`, which says how its request met the cache, as the real CLI's does.
pub(crate) fn message_start(turn: &Turn, message_id: &str) {
    let message = json!({"id":message_id,"model":"fake-model","role":"assistant","usage":usage(turn.parent)});
    let event = json!({"type":"message_start","message":message});
    emit(
        turn.out,
        json!({"type":"stream_event","event":event,"session_id":turn.config.session_id,
        "parent_tool_use_id":turn.parent,"api_message_id":message_id,"ttft_ms":FIRST_TOKEN_MS}),
    );
}

/// What a message's request read and wrote: the conversation read nearly whole and written where it grew, at 1h as the
/// agent's own are, or written whole once `cold` has said to. `parent` is the call whose subagent it's from, if any.
fn usage(parent: Option<&str>) -> Value {
    let (read, written) =
        if parent.is_none() && std::mem::take(&mut *COLD.lock()) { (14_800, 238_000) } else { (100_000, 1_000) };
    json!({"input_tokens":2,"cache_creation_input_tokens":written,"cache_read_input_tokens":read,
        "cache_creation":{"ephemeral_5m_input_tokens":0,"ephemeral_1h_input_tokens":written},"output_tokens":1})
}
