//! On-demand conversation compaction via `agy acp`'s `/compact` slash command over `session/prompt`.
//!
//! `agy` 1.5.12 has no dedicated compaction JSON-RPC method. Sending bare `/compact` triggers a single summarization
//! model call (writing a `CHECKPOINT` to `transcript.jsonl`) with no `agent_message_chunk` output and resolves with
//! `stopReason: "end_turn"`. Trailing text after `/compact` would trigger a second full model turn after compacting, so
//! the adapter claims `Keeping::Ignored` and always sends bare `/compact`.

use std::sync::Arc;
use std::time::Instant;

use protocol::event::{AgentRunState, CompactedBy, CompactionOutcome, EventBody, TurnOutcome};
use protocol::model::Seq;
use serde_json::{Value, json};

use super::control::{between_endings, write_prompt_request};
use super::notifications;
use super::running::Shared;
use super::state::{InFlightPrompt, PromptOrigin};
use super::swap::ensure_mode_process;
use super::turn;
use crate::AgentError;
use crate::acp::{self, StopReason};

/// Asks `agy` to compact the conversation between turns, answering the hub's `CompactionAsked` at `asked`.
pub(super) async fn request(shared: &Arc<Shared>, asked: Seq) -> Result<(), AgentError> {
    between_endings(shared, |state| {
        if state.in_turn() || !state.waiting.is_empty() {
            return Err(AgentError::Refused("Antigravity compacts only between turns".into()));
        }
        Ok(())
    })
    .await?;
    ensure_mode_process(shared).await?;
    notifications::flush_streams(shared).await;
    let rpc = shared.rpc();
    let ((session_id, rpc_id), _) = between_endings(shared, |state| {
        if state.in_turn() || !state.waiting.is_empty() {
            return Err(AgentError::Refused("Antigravity compacts only between turns".into()));
        }
        let session_id = state.session_id.clone().ok_or(AgentError::Exited)?;
        state.reply = None;
        state.cancelling = false;
        let rpc_id = rpc.next_id();
        state.prompts.push_back(InFlightPrompt {
            rpc_id,
            origin: PromptOrigin::Compact { asked, started_at: Instant::now() },
            taken: true,
            user_chunks_left: 1,
        });
        Ok((session_id, rpc_id))
    })
    .await?;
    shared.event(EventBody::PromptTaken { prompt: asked }, None).await;
    shared.event(EventBody::AgentState { state: AgentRunState::Running }, None).await;
    shared.event(EventBody::AgentCompacting { written_chars: None }, None).await;
    write_prompt_request(shared, &rpc, rpc_id, &session_id, vec![json!({ "type": "text", "text": "/compact" })]).await;
    Ok(())
}

/// Settles a `/compact` `session/prompt` that returned an ACP JSON-RPC result.
pub(super) async fn finish_ok(shared: &Arc<Shared>, asked: Seq, started_at: Instant, result: &Value, raw: String) {
    let parsed = acp::parse_prompt_response(result);
    let (was_cancelling, reply) = {
        let mut state = shared.state.lock();
        let was = state.cancelling;
        state.cancelling = false;
        (was, state.reply.take())
    };
    let cancelled = was_cancelling || parsed.stop_reason == StopReason::Cancelled;
    if cancelled {
        turn::settle_turn(shared).await;
        turn::record_compacted_end(shared, TurnOutcome::Cancelled, None, parsed.tokens, false, Some(raw)).await;
        return;
    }
    let error_text = reply.map(|text| text.trim().to_owned()).filter(|text| !text.is_empty());
    let outcome = if let Some(error) = error_text {
        CompactionOutcome::Failed { error: Some(error) }
    } else if parsed.stop_reason != StopReason::EndTurn {
        CompactionOutcome::Failed { error: None }
    } else {
        let took_ms = u64::try_from(started_at.elapsed().as_millis()).ok();
        CompactionOutcome::Compacted {
            by: CompactedBy::Asked,
            before_tokens: None,
            after_tokens: None,
            took_ms,
            summary: None,
        }
    };
    shared.event(EventBody::AgentCompacted { asked: Some(asked), keeping: None, outcome }, None).await;
    turn::settle_turn(shared).await;
    let turn_outcome = parsed.stop_reason.outcome();
    let only_compacted = turn_outcome == TurnOutcome::Success;
    turn::record_compacted_end(shared, turn_outcome, None, parsed.tokens, only_compacted, Some(raw)).await;
}

/// Settles a `/compact` `session/prompt` that returned a JSON-RPC error.
pub(super) async fn finish_err(shared: &Arc<Shared>, asked: Seq, error: &AgentError, raw: String) {
    {
        let mut state = shared.state.lock();
        state.cancelling = false;
        state.reply = None;
    }
    let outcome = CompactionOutcome::Failed { error: Some(error.to_string()) };
    shared.event(EventBody::AgentCompacted { asked: Some(asked), keeping: None, outcome }, None).await;
    turn::settle_turn(shared).await;
    turn::record_compacted_end(shared, TurnOutcome::Error, Some(error.to_string()), None, false, Some(raw)).await;
}
