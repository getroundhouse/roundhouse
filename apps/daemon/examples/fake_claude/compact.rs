//! A typed `/compact`, as the real CLI answers one: a turn of its own that says it's compacting, that it went well,
//! then the boundary with what it freed, the summary for the model, and `Compacted`, and nothing else.

use serde_json::json;

use crate::turn::{begin, state_event};
use crate::{Config, Out, Prompt, emit, lifecycle};

/// Compacts, if `prompt` is `/compact`, and says whether it was.
pub(crate) fn if_asked(config: &Config, out: &Out, prompt: &Prompt) -> bool {
    let Some(keeping) = asked(&prompt.text) else { return false };
    compact(config, out, prompt, keeping);
    true
}

/// What follows `/compact` in `text`, which is what to keep, if it's a compaction.
fn asked(text: &str) -> Option<&str> {
    let rest = text.trim().strip_prefix("/compact")?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim())
}

/// Compacts, with a summary that says what it was told to keep.
fn compact(config: &Config, out: &Out, prompt: &Prompt, keeping: &str) {
    let session_id = &config.session_id;
    state_event(config, out, "running");
    lifecycle(out, prompt, "started");
    emit(out, json!({"type":"system","subtype":"status","status":"compacting","session_id":session_id}));
    emit(
        out,
        json!({"type":"system","subtype":"status","status":null,"compact_result":"success","session_id":session_id}),
    );
    // The real CLI starts again before the boundary.
    begin(config, out);
    emit(
        out,
        json!({"type":"system","subtype":"compact_boundary","session_id":session_id,"compact_metadata":
            {"trigger":"manual","pre_tokens":16_000,"post_tokens":4_000,"duration_ms":1_500}}),
    );
    let kept = if keeping.is_empty() { String::new() } else { format!("\n2. What to keep: {keeping}") };
    let summary = format!(
        "This session is being continued from a previous conversation.\nSummary:\n1. What was asked: the prompts \
         so far.{kept}\n\nIf you need specific details from before compaction, read the transcript."
    );
    emit(
        out,
        json!({"type":"user","isSynthetic":true,"message":{"role":"user","content":summary},
            "parent_tool_use_id":null,"session_id":session_id}),
    );
    emit(
        out,
        json!({"type":"user","isReplay":true,"message":{"role":"user",
            "content":"<local-command-stdout>Compacted </local-command-stdout>"},"session_id":session_id}),
    );
    emit(
        out,
        json!({"type":"result","subtype":"success","is_error":false,"result":"","duration_ms":1_500,"num_turns":0,
            "total_cost_usd":0.0,"session_id":session_id}),
    );
    lifecycle(out, prompt, "completed");
    state_event(config, out, "idle");
}
