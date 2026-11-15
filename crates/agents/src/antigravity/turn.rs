use protocol::event::{EventBody, NoticeLevel, TokenUsage, TurnOutcome};
use protocol::model::PermissionMode;
use serde_json::Value;

use super::state::{Ending, InFlightPrompt};
use super::{approvals, control, notifications, running::Shared};
use crate::acp::{self, StopReason};
use crate::jsonrpc::RpcReply;

pub(super) async fn prompt_finished(shared: &Shared, rpc_id: u64, reply: RpcReply, raw: String) {
    let (entry, has_remaining, was_cancelling) = {
        let mut state = shared.state.lock();
        let Some(pos) = state.prompts.iter().position(|p| p.rpc_id == rpc_id) else { return };
        let entry = state.prompts.remove(pos);
        let has_remaining = !state.prompts.is_empty();
        if !has_remaining {
            state.ending = Ending::Recording;
        }
        (entry, has_remaining, state.cancelling)
    };
    let Some(entry) = entry else { return };
    match (reply, has_remaining) {
        (Ok(result), true) => {
            if !entry.taken
                && let Some(prompt) = entry.seq
            {
                let cancelled =
                    was_cancelling || acp::parse_prompt_response(&result).stop_reason == StopReason::Cancelled;
                let event =
                    if cancelled { EventBody::PromptDropped { prompt } } else { EventBody::PromptTaken { prompt } };
                shared.event(event, None).await;
            }
        }
        (Err(error), true) => {
            if !entry.taken
                && let Some(prompt) = entry.seq
            {
                shared.event(EventBody::PromptDropped { prompt }, None).await;
            }
            shared
                .event(
                    EventBody::Notice {
                        level: NoticeLevel::Error,
                        message: error.to_string(),
                        parent_tool_use_id: None,
                    },
                    Some(raw),
                )
                .await;
        }
        (Err(error), false) => {
            notifications::flush_streams(shared).await;
            if !entry.taken
                && let Some(prompt) = entry.seq
            {
                shared.event(EventBody::PromptDropped { prompt }, None).await;
            }
            shared.state.lock().cancelling = false;
            control::settle_all(shared).await;
            record_end(shared, TurnOutcome::Error, Some(error.to_string()), None, Some(raw)).await;
        }
        (Ok(result), false) => finish_last_ok(shared, entry, &result, raw).await,
    }
}

async fn finish_last_ok(shared: &Shared, entry: InFlightPrompt, result: &Value, raw: String) {
    notifications::flush_streams(shared).await;
    let parsed = acp::parse_prompt_response(result);
    let was_cancelling = {
        let mut state = shared.state.lock();
        let was = state.cancelling;
        state.cancelling = false;
        was
    };
    let cancelled = was_cancelling || parsed.stop_reason == StopReason::Cancelled;
    if !entry.taken
        && let Some(prompt) = entry.seq
    {
        let event = if cancelled { EventBody::PromptDropped { prompt } } else { EventBody::PromptTaken { prompt } };
        shared.event(event, None).await;
    }
    let held_plan = if !cancelled && parsed.stop_reason == StopReason::EndTurn {
        let mut state = shared.state.lock();
        let plan = if state.hold_plans && state.mode == PermissionMode::Plan && state.waiting.is_empty() {
            state.reply.take().filter(|text| !text.trim().is_empty())
        } else {
            None
        };
        if plan.is_some() {
            state.add_tokens(parsed.tokens);
        }
        plan
    } else {
        None
    };
    if let Some(plan) = held_plan {
        approvals::ask_plan(shared, plan).await;
        shared.count_end(&mut shared.state.lock(), Ending::HeldForPlan);
        return;
    }
    control::settle_turn(shared).await;
    let outcome = if cancelled { TurnOutcome::Cancelled } else { parsed.stop_reason.outcome() };
    record_end(shared, outcome, None, parsed.tokens, Some(raw)).await;
}

pub(super) async fn record_end(
    shared: &Shared,
    outcome: TurnOutcome,
    error: Option<String>,
    tokens: Option<TokenUsage>,
    raw: Option<String>,
) {
    let (reply, model, tokens) = {
        let mut state = shared.state.lock();
        state.add_tokens(tokens);
        (state.reply.take(), state.model.clone(), state.earlier_tokens.take())
    };
    let models = if outcome == TurnOutcome::Cancelled && reply.is_none() { Vec::new() } else { vec![model] };
    shared
        .event(
            EventBody::TurnEnded {
                outcome,
                result: reply.filter(|_| outcome != TurnOutcome::Cancelled),
                errors: error.into_iter().collect(),
                duration_ms: None,
                cost_usd: None,
                num_turns: None,
                started_at_ms: None,
                models,
                tokens,
                only_compacted: false,
                request_failed: false,
            },
            raw,
        )
        .await;
    // Counted once `TurnEnded` is out, so whatever waits on the end finds it told, and before the state is reported,
    // which reads it.
    shared.count_end(&mut shared.state.lock(), Ending::Not);
    control::report_state(shared).await;
}
