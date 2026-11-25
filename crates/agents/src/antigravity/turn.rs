use std::sync::Arc;

use protocol::event::{AgentRunState, EventBody, NoticeLevel, TokenUsage, TurnOutcome};
use protocol::model::Seq;
use serde_json::Value;

use super::state::{Ending, InFlightPrompt};
use super::{approvals, notifications, running::Shared};
use crate::acp::{self, StopReason};
use crate::jsonrpc::RpcReply;
use tracing::Instrument as _;

pub(super) async fn prompt_finished(shared: &Arc<Shared>, rpc_id: u64, reply: RpcReply, raw: String) {
    let transcript = read_turn_transcript(shared).await;
    let (entry, has_remaining, was_cancelling) = {
        let mut guard = shared.state.lock();
        let state = &mut *guard;
        let Some(pos) = state.prompts.iter().position(|p| p.rpc_id == rpc_id) else { return };
        let entry = state.prompts.remove(pos);
        let has_remaining = !state.prompts.is_empty();
        if !has_remaining {
            state.ending = Ending::Recording;
            apply_turn_transcript(state, transcript);
        }
        (entry, has_remaining, state.cancelling)
    };
    let Some(entry) = entry else { return };
    match (reply, has_remaining) {
        (Ok(result), true) => {
            if !entry.taken
                && let Some(prompt) = entry.seq()
            {
                notifications::flush_streams(shared).await;
                let cancelled =
                    was_cancelling || acp::parse_prompt_response(&result).stop_reason == StopReason::Cancelled;
                if !cancelled {
                    shared.state.lock().reply = None;
                }
                let event =
                    if cancelled { EventBody::PromptDropped { prompt } } else { EventBody::PromptTaken { prompt } };
                shared.run.event(event, None).await;
            }
        }
        (Err(error), true) => {
            if !entry.taken
                && let Some(prompt) = entry.seq()
            {
                shared.run.event(EventBody::PromptDropped { prompt }, None).await;
            }
            shared
                .run
                .event(
                    EventBody::Notice { level: NoticeLevel::Error, message: error.to_string(), subagent: None },
                    Some(raw),
                )
                .await;
        }
        (reply, false) => finish_last(shared, &entry, reply, raw).await,
    }
}

async fn finish_last(shared: &Arc<Shared>, entry: &InFlightPrompt, reply: RpcReply, raw: String) {
    match (reply, entry.origin) {
        (Err(error), super::state::PromptOrigin::Compact { asked, .. }) => {
            super::compact::finish_err(shared, asked, &error, raw).await;
        }
        (Ok(result), super::state::PromptOrigin::Compact { asked, started_at }) => {
            super::compact::finish_ok(shared, asked, started_at, &result, raw).await;
        }
        (Err(error), super::state::PromptOrigin::Prompt(_) | super::state::PromptOrigin::WakeupDrain { .. }) => {
            finish_last_err(shared, entry, &error, raw).await;
        }
        (Ok(result), super::state::PromptOrigin::Prompt(_) | super::state::PromptOrigin::WakeupDrain { .. }) => {
            finish_last_ok(shared, entry, &result, raw).await;
        }
    }
}

enum TurnTranscript {
    None,
    /// Bytes read starting at `from` while live subagents exist; only complete lines advance the cursors.
    Read {
        from: u64,
        bytes: Vec<u8>,
    },
    /// No live subagents: advance both cursors to the file's settled length.
    AdvanceTo(u64),
}

async fn read_turn_transcript(shared: &Shared) -> TurnTranscript {
    let scan = {
        let state = shared.state.lock();
        state
            .transcript
            .as_ref()
            .map(|cursor| (cursor.path.clone(), cursor.async_agents_offset, state.async_agents.has_live_subagents()))
    };
    match scan {
        Some((path, from, true)) => {
            let bytes = super::wakeup::read_since(&path, from).await.unwrap_or_default();
            TurnTranscript::Read { from, bytes }
        }
        Some((path, _, false)) => tokio::fs::metadata(path)
            .await
            .ok()
            .map_or(TurnTranscript::None, |meta| TurnTranscript::AdvanceTo(meta.len())),
        None => TurnTranscript::None,
    }
}

fn apply_turn_transcript(state: &mut super::state::Conversation, transcript: TurnTranscript) {
    let Some(cursor) = state.transcript.as_mut() else { return };
    match transcript {
        TurnTranscript::Read { from, bytes } => {
            let skip = usize::try_from(cursor.async_agents_offset.saturating_sub(from)).unwrap_or(bytes.len());
            let slice = bytes.get(skip..).unwrap_or_default();
            let consumed = state.async_agents.on_transcript_bytes(slice);
            let next = from
                .saturating_add(u64::try_from(skip).unwrap_or(u64::MAX))
                .saturating_add(u64::try_from(consumed).unwrap_or(u64::MAX));
            cursor.advance_both(next);
        }
        TurnTranscript::AdvanceTo(len) => cursor.advance_both(len),
        TurnTranscript::None => {}
    }
}

async fn finish_last_err(shared: &Arc<Shared>, entry: &InFlightPrompt, error: &crate::AgentError, raw: String) {
    notifications::flush_streams(shared).await;
    if !entry.taken {
        shared.state.lock().reply = None;
        if let Some(prompt) = entry.seq() {
            shared.run.event(EventBody::PromptDropped { prompt }, None).await;
        }
    }
    shared.state.lock().cancelling = false;
    if entry.is_drain() && !entry.started_own_turn() {
        let why = shared.state.lock().take_ended_own_turn();
        let Some(why) = why else {
            shared
                .run
                .event(
                    EventBody::Notice { level: NoticeLevel::Error, message: error.to_string(), subagent: None },
                    Some(raw),
                )
                .await;
            settle_turn(shared).await;
            shared.count_end(&mut shared.state.lock(), Ending::Not);
            report_state(shared).await;
            schedule_pending_mode_swap(shared);
            return;
        };
        shared.run.event(EventBody::AgentState { state: AgentRunState::Running }, None).await;
        shared.run.event(EventBody::OwnTurn(why), None).await;
    }
    settle_turn(shared).await;
    record_end(shared, TurnOutcome::Error, Some(error.to_string()), None, Some(raw)).await;
}

async fn finish_last_ok(shared: &Arc<Shared>, entry: &InFlightPrompt, result: &Value, raw: String) {
    notifications::flush_streams(shared).await;
    let parsed = acp::parse_prompt_response(result);
    let was_cancelling = {
        let mut state = shared.state.lock();
        let was = state.cancelling;
        state.cancelling = false;
        was
    };
    let self_cancelled = !was_cancelling && entry.self_cancelled();
    let cancelled = was_cancelling || (!self_cancelled && parsed.stop_reason == StopReason::Cancelled);
    if !entry.taken
        && let Some(prompt) = entry.seq()
    {
        let event = if cancelled { EventBody::PromptDropped { prompt } } else { EventBody::PromptTaken { prompt } };
        shared.run.event(event, None).await;
    }
    if entry.is_drain() && !entry.started_own_turn() {
        settle_turn(shared).await;
        shared.count_end(&mut shared.state.lock(), Ending::Not);
        report_state(shared).await;
        schedule_pending_mode_swap(shared);
        return;
    }
    let ended_normally = self_cancelled || parsed.stop_reason == StopReason::EndTurn;
    let held_plan =
        if !cancelled && ended_normally { take_held_plan(&mut shared.state.lock(), parsed.tokens) } else { None };
    if let Some(plan) = held_plan {
        approvals::ask_plan(shared, plan).await;
        shared.count_end(&mut shared.state.lock(), Ending::HeldForPlan);
        return;
    }
    settle_turn(shared).await;
    let outcome = if cancelled {
        TurnOutcome::Cancelled
    } else if self_cancelled {
        TurnOutcome::Success
    } else {
        parsed.stop_reason.outcome()
    };
    let end_raw = (!self_cancelled).then_some(raw);
    record_end(shared, outcome, None, parsed.tokens, end_raw).await;
}

/// Settles a between-turn idle `OwnTurn` streamed natively by `agy` 1.5.12+ once `transcript.jsonl` records its final
/// `PLANNER_RESPONSE` or `ERROR_MESSAGE`. Returns `true` when the wakeup was consumed so the watcher returns to `Idle`.
pub(super) async fn settle_idle_wakeup(shared: &Arc<Shared>, outcome: TurnOutcome, watch_offset: u64) -> bool {
    let _ordered = shared.controls.lock().await;
    let active = {
        let mut state = shared.state.lock();
        if !state.prompts.is_empty() || state.ending != Ending::Not || !state.waiting.is_empty() {
            return false;
        }
        if let Some(cursor) = state.transcript.as_mut() {
            cursor.wakeup_floor = cursor.wakeup_floor.max(watch_offset);
        }
        let active = state.idle_own_turn == super::wakeup::IdleOwnTurn::Active;
        if active {
            state.idle_own_turn = super::wakeup::IdleOwnTurn::None;
            state.ending = Ending::Recording;
        }
        active
    };
    if !active {
        return true;
    }
    notifications::flush_streams(shared).await;
    let held_plan = (outcome == TurnOutcome::Success).then(|| take_held_plan(&mut shared.state.lock(), None)).flatten();
    if let Some(plan) = held_plan {
        approvals::ask_plan(shared, plan).await;
        shared.count_end(&mut shared.state.lock(), Ending::HeldForPlan);
        return true;
    }
    settle_turn(shared).await;
    record_end(shared, outcome, None, None, None).await;
    true
}

fn take_held_plan(state: &mut super::state::Conversation, tokens: Option<TokenUsage>) -> Option<String> {
    let plan =
        if state.hold_plans && state.process_mode.plan && state.waiting.is_empty() && !state.awaiting_background_work()
        {
            state.reply.take().map(|text| text.trim().to_owned()).filter(|text| !text.is_empty())
        } else {
            None
        };
    if plan.is_some() {
        state.add_tokens(tokens);
    }
    plan
}

pub(super) async fn record_end(
    shared: &Arc<Shared>,
    outcome: TurnOutcome,
    error: Option<String>,
    tokens: Option<TokenUsage>,
    raw: Option<String>,
) {
    record_compacted_end(shared, outcome, error, tokens, false, raw).await;
}

pub(super) async fn record_compacted_end(
    shared: &Arc<Shared>,
    outcome: TurnOutcome,
    error: Option<String>,
    tokens: Option<TokenUsage>,
    only_compacted: bool,
    raw: Option<String>,
) {
    let (reply, model, tokens) = {
        let mut state = shared.state.lock();
        state.idle_own_turn = super::wakeup::IdleOwnTurn::None;
        state.add_tokens(tokens);
        (state.reply.take(), state.model.clone(), state.earlier_tokens.take())
    };
    let models = if outcome == TurnOutcome::Cancelled && reply.is_none() { Vec::new() } else { vec![model] };
    shared
        .run
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
                only_compacted,
                request_failed: false,
            },
            raw,
        )
        .await;
    // Counted once `TurnEnded` is out, so whatever waits on the end finds it told, and before the state is reported,
    // which reads it.
    shared.count_end(&mut shared.state.lock(), Ending::Not);
    report_state(shared).await;
    schedule_pending_mode_swap(shared);
}

fn schedule_pending_mode_swap(shared: &Arc<Shared>) {
    if !shared.state.lock().needs_mode_swap() {
        return;
    }
    let shared = shared.clone();
    let span = shared.run.span();
    tokio::spawn(
        async move {
            let _ordered = shared.controls.lock().await;
            let _ = super::swap::ensure_mode_process(&shared).await;
        }
        .instrument(span),
    );
}

pub(super) async fn settle_replaced_child(shared: &Shared) {
    notifications::flush_streams(shared).await;
    let (dropped, stopped) = shared.state.lock().take_replaced_child_tasks();
    for tool_use_id in dropped {
        shared.run.event(EventBody::ToolCallDropped { tool_use_id }, None).await;
    }
    if let Some(stopped) = stopped {
        shared.run.event(EventBody::AgentTasks { tasks: Vec::new(), stopped }, None).await;
    }
}

pub(super) async fn settle_all(shared: &Shared) {
    let prompts: Vec<Seq> = {
        let mut state = shared.state.lock();
        state.idle_own_turn = super::wakeup::IdleOwnTurn::None;
        for open in state.calls.values_mut() {
            open.outlives_turn = false;
        }
        state.prompts.drain(..).filter(|entry| !entry.taken).filter_map(|entry| entry.seq()).collect()
    };
    for prompt in prompts {
        shared.run.event(EventBody::PromptDropped { prompt }, None).await;
    }
    settle_turn(shared).await;
}

pub(super) async fn settle_turn(shared: &Shared) {
    let (requests, calls, surviving_tasks) = {
        let mut guard = shared.state.lock();
        let state = &mut *guard;
        let requests: Vec<_> = state.waiting.drain().map(|(id, _)| id).collect();
        let mut calls = Vec::new();
        state.calls.retain(|id, open| {
            if open.outlives_turn {
                true
            } else {
                calls.push(id.clone());
                false
            }
        });
        state.denied_calls.retain(|id| state.calls.contains_key(id));
        state.async_agents.on_turn_settled();
        let surviving_tasks = state.list_surviving_commands();
        (requests, calls, surviving_tasks)
    };
    for approval_id in requests {
        shared.run.event(EventBody::ApprovalResolved { approval_id, decision: None, by_setting: false }, None).await;
    }
    for tool_use_id in calls {
        shared.run.event(EventBody::ToolCallDropped { tool_use_id }, None).await;
    }
    if let Some(tasks) = surviving_tasks {
        shared.run.event(EventBody::AgentTasks { tasks, stopped: Vec::new() }, None).await;
    }
}

pub(super) async fn report_state(shared: &Shared) {
    let state = {
        let state = shared.state.lock();
        if !state.waiting.is_empty() {
            AgentRunState::RequiresAction
        } else if state.in_turn() {
            AgentRunState::Running
        } else {
            AgentRunState::Idle
        }
    };
    shared.run.event(EventBody::AgentState { state }, None).await;
}
