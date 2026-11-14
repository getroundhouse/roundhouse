use std::sync::Arc;

use protocol::event::{AgentRunState, EventBody, NoticeLevel, TurnOutcome};
use protocol::tail::{DeltaChange, StreamDelta};
use serde_json::Value;
use tokio::sync::mpsc;

use super::{approvals, control, items, running::Shared, state::TurnId, subagents};
use crate::{AgentError, AgentOutput};

pub(super) async fn read(shared: Arc<Shared>, mut input: mpsc::Receiver<(Value, String)>) {
    while let Some((value, raw)) = input.recv().await {
        if value.get("method").is_none() {
            shared.rpc.answered(&value);
            continue;
        }
        if let Err(error) = notification(&shared, &value, raw).await {
            shared
                .event(
                    EventBody::Notice {
                        level: NoticeLevel::Error,
                        message: format!("Could not read Codex's message: {error}"),
                        parent_tool_use_id: subagents::scope(&shared, &value["params"]).and_then(|scope| scope.parent),
                    },
                    None,
                )
                .await;
        }
    }
}

async fn notification(shared: &Shared, value: &Value, raw: String) -> Result<(), AgentError> {
    let method = value["method"].as_str().unwrap_or_default();
    let params = &value["params"];
    // app-server also emits its older event protocol. The v2 item stream is authoritative.
    if method.starts_with("codex/event/") {
        return Ok(());
    }
    if method == "turn/started" {
        subagents::turn_started(shared, params).await?;
    }
    let Some(scope) = subagents::scope(shared, params) else {
        // Preserve unsupported foreign-thread data without letting it consume a prompt, settle the root turn,
        // or appear as the root assistant. No call association means it cannot be normalized safely.
        shared
            .event(
                EventBody::AgentOther {
                    kind: format!("unassociated Codex thread: {method}"),
                    parent_tool_use_id: params["threadId"].as_str().map(|id| format!("codex-thread-{id}").into()),
                },
                Some(raw),
            )
            .await;
        if value.get("id").is_some() {
            approvals::unsupported(shared, &value["id"], method).await?;
        }
        return Ok(());
    };
    if value.get("id").is_some() {
        return approvals::requested(shared, method, &value["id"], params, &scope, raw).await;
    }
    dispatch(shared, method, params, &scope, raw).await
}

async fn dispatch(
    shared: &Shared,
    method: &str,
    params: &Value,
    scope: &subagents::Scope,
    raw: String,
) -> Result<(), AgentError> {
    match method {
        "turn/started" => {
            if scope.parent.is_some() {
                return Ok(());
            }
            let id: TurnId = serde_json::from_value(params["turn"]["id"].clone())?;
            shared.state.lock().turn = Some(id);
            shared.state.lock().reply = None;
            shared.state.lock().plan = None;
            shared.event(EventBody::AgentState { state: AgentRunState::Running }, Some(raw)).await;
        }
        "turn/completed" => {
            if let Some(call) = &scope.parent {
                subagents::complete(shared, call, &params["turn"], raw).await;
            } else {
                complete(shared, &params["turn"], raw).await?;
            }
        }
        "item/started" | "item/completed" => {
            items::Reader { shared, scope }.item(&params["item"], method == "item/completed", raw).await?;
        }
        "item/agentMessage/delta" | "item/plan/delta" | "item/reasoning/summaryTextDelta" => {
            delta(shared, method, params, scope).await;
        }
        "serverRequest/resolved" => approvals::resolved(shared, &params["requestId"]).await,
        "error" => {
            let message = params["error"]["message"].as_str().unwrap_or("Codex's request failed");
            shared
                .event(
                    EventBody::Notice {
                        level: NoticeLevel::Error,
                        message: message.into(),
                        parent_tool_use_id: scope.parent.clone(),
                    },
                    Some(raw),
                )
                .await;
        }
        // Idle can precede turn/completed. Only the normalized outcome makes Roundhouse idle.
        "thread/status/changed" | "thread/started" | "item/reasoning/textDelta" => {}
        _ => {
            shared
                .event(
                    EventBody::AgentOther { kind: method.into(), parent_tool_use_id: scope.parent.clone() },
                    Some(raw),
                )
                .await;
        }
    }
    Ok(())
}

async fn delta(shared: &Shared, method: &str, params: &Value, scope: &subagents::Scope) {
    let Some(id) = params["itemId"].as_str() else { return };
    let Some(text) = params["delta"].as_str() else { return };
    let change = if method.starts_with("item/reasoning/") {
        DeltaChange::Thinking { index: 0, text: text.into() }
    } else {
        DeltaChange::Text { index: 0, text: text.into() }
    };
    shared
        .emit(AgentOutput::Delta(StreamDelta {
            run: None,
            message_id: id.into(),
            parent_tool_use_id: scope.parent.clone(),
            change,
        }))
        .await;
}

async fn complete(shared: &Shared, turn: &Value, raw: String) -> Result<(), AgentError> {
    let id: TurnId = serde_json::from_value(turn["id"].clone())?;
    let (plan, cancelled) = {
        let mut state = shared.state.lock();
        if state.turn.as_ref() != Some(&id) {
            return Ok(());
        }
        if state.cancelling {
            shared.turn_done.send_replace(Some(id));
            return Ok(());
        }
        state.turn = None;
        (state.take_plan(), turn["status"] == "interrupted")
    };
    if turn["status"] == "completed"
        && let Some(plan) = plan
    {
        approvals::plan(shared, plan).await;
        shared.turn_done.send_replace(Some(id));
        return Ok(());
    }
    control::settle(shared, None).await;
    let outcome = if cancelled {
        TurnOutcome::Cancelled
    } else if turn["status"] == "completed" {
        TurnOutcome::Success
    } else {
        TurnOutcome::Error
    };
    let error = turn["error"]["message"].as_str().map(str::to_owned);
    let mut end = ended(outcome, error);
    if let EventBody::TurnEnded { result, .. } = &mut end {
        *result = shared.state.lock().reply.take();
    }
    shared.event(end, Some(raw)).await;
    control::report_state(shared).await;
    shared.turn_done.send_replace(Some(id));
    Ok(())
}

pub(super) fn ended(outcome: TurnOutcome, error: Option<String>) -> EventBody {
    EventBody::TurnEnded {
        outcome,
        result: None,
        errors: error.into_iter().collect(),
        duration_ms: None,
        cost_usd: None,
        num_turns: None,
        started_at_ms: None,
        models: Vec::new(),
        tokens: None,
        only_compacted: false,
        request_failed: false,
    }
}
