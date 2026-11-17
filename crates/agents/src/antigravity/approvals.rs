use std::sync::Arc;

use protocol::capability::{
    approvals::{AfterDenial, Asks, Decision, Remember, Request},
    asking::PlanContext,
};
use protocol::event::{AgentRunState, EventBody, NoticeLevel, TurnOutcome};
use protocol::model::{ApprovalId, PermissionMode, ToolUseId};
use serde_json::{Value, json};

use super::{
    calls, control, notifications,
    running::Shared,
    state::{Ending, NativeRequest, Waiting},
    turn,
};
use crate::{AgentError, acp, jsonrpc};

pub(super) async fn requested(
    shared: &Arc<Shared>,
    method: &str,
    native_id: &Value,
    params: &Value,
    raw: String,
) -> Result<(), AgentError> {
    match method {
        "session/request_permission" => permission_requested(shared, native_id, params, raw).await,
        "elicitation/create" => elicitation_requested(shared, native_id, params, raw).await,
        _ => unsupported(shared, native_id, method).await,
    }
}

async fn permission_requested(
    shared: &Shared,
    native_id: &Value,
    params: &Value,
    raw: String,
) -> Result<(), AgentError> {
    let Some(perm) = acp::parse_permission_request(params) else {
        return unsupported(shared, native_id, "session/request_permission").await;
    };
    let id = ToolUseId::from(perm.tool_call.tool_call_id.as_str());
    let (mode, in_turn) = {
        let mut state = shared.state.lock();
        if let Some(open) = state.calls.get_mut(&id) {
            open.outlives_turn = false;
        }
        let in_turn = state.in_turn();
        if in_turn {
            state.note_wakeup_interaction();
        }
        (state.process_mode, in_turn)
    };
    if !in_turn {
        let reject = perm.option_for(false, false).unwrap_or("reject");
        return shared.rpc().reply(native_id.clone(), acp::permission_selected(reject)).await;
    }
    notifications::ensure_own_turn_started(shared).await;
    if mode == PermissionMode::FullAccess {
        let allow = perm.option_for(false, true).unwrap_or("allow");
        return shared.rpc().reply(native_id.clone(), acp::permission_selected(allow)).await;
    }
    notifications::ensure_call_emitted(shared, &perm.tool_call, None).await;
    let call = {
        let mut state = shared.state.lock();
        if let Some(open) = state.calls.get_mut(&id) {
            open.outlives_turn = false;
            open.call.clone()
        } else {
            calls::translate_call(&perm.tool_call)
        }
    };
    let request = Request { asks: Asks::Call { call }, suggested: Vec::new(), always: false, why: None };
    // A call's request names the call, as Claude's and Codex's do, so a client ties the call's result to it.
    ask(
        shared,
        ApprovalId::from(format!("agy-{}", uuid::Uuid::new_v4())),
        request,
        NativeRequest::Permission { id: native_id.clone(), request: Box::new(perm) },
        Some(id),
        Some(raw),
    )
    .await;
    Ok(())
}

async fn elicitation_requested(
    shared: &Shared,
    native_id: &Value,
    params: &Value,
    raw: String,
) -> Result<(), AgentError> {
    let in_turn = {
        let mut state = shared.state.lock();
        let in_turn = state.in_turn();
        if in_turn {
            state.note_wakeup_interaction();
        }
        in_turn
    };
    if !in_turn {
        return shared.rpc().reply(native_id.clone(), acp::elicitation_cancel()).await;
    }
    notifications::ensure_own_turn_started(shared).await;
    let Some(form) = acp::parse_elicitation(params) else {
        return unsupported(shared, native_id, "elicitation/create").await;
    };
    let request = Request {
        asks: Asks::Questions { questions: form.questions },
        suggested: Vec::new(),
        always: false,
        why: None,
    };
    ask(
        shared,
        ApprovalId::from(format!("agy-ask-{}", uuid::Uuid::new_v4())),
        request,
        NativeRequest::Elicitation { id: native_id.clone(), fields: form.fields },
        None,
        Some(raw),
    )
    .await;
    Ok(())
}

pub(super) async fn ask_plan(shared: &Shared, plan: String) {
    let id = ApprovalId::from(format!("agy-plan-{}", uuid::Uuid::new_v4()));
    let request = Request { asks: Asks::Plan { plan, file: None }, suggested: Vec::new(), always: false, why: None };
    ask(shared, id, request, NativeRequest::Plan, None, None).await;
}

/// Records `request` as waiting on the person, as `id`, naming the call it asks to make, if it asks to make one.
async fn ask(
    shared: &Shared,
    id: ApprovalId,
    request: Request,
    native: NativeRequest,
    call: Option<ToolUseId>,
    raw: Option<String>,
) {
    shared.state.lock().waiting.insert(id.clone(), Waiting { native });
    shared
        .event(
            EventBody::ApprovalRequested {
                approval_id: id,
                request: Box::new(request),
                tool_use_id: call,
                asked_by: None,
            },
            raw,
        )
        .await;
    shared.event(EventBody::AgentState { state: AgentRunState::RequiresAction }, None).await;
}

pub(super) async fn cancel_native_waiters(shared: &Shared) {
    let replies: Vec<(Value, Value)> = {
        let state = shared.state.lock();
        state
            .waiting
            .values()
            .filter_map(|waiting| match &waiting.native {
                NativeRequest::Permission { id, .. } => Some((id.clone(), acp::permission_cancelled())),
                NativeRequest::Elicitation { id, .. } => Some((id.clone(), acp::elicitation_cancel())),
                NativeRequest::Plan => None,
            })
            .collect()
    };
    let rpc = shared.rpc();
    for (id, payload) in replies {
        let _ = rpc.reply(id, payload).await;
    }
}

pub(super) async fn decide(shared: &Arc<Shared>, id: &ApprovalId, decision: &Decision) -> Result<(), AgentError> {
    let waiting = shared
        .state
        .lock()
        .waiting
        .remove(id)
        .ok_or_else(|| AgentError::Refused("That Antigravity request is no longer open".into()))?;
    if !can_answer(&waiting.native, decision) {
        shared.state.lock().waiting.insert(id.clone(), waiting);
        return Err(unsupported_decision());
    }
    if let Err(error) = answer(shared, &waiting, decision).await {
        let refused = jsonrpc::refused(&error);
        if refused && matches!(waiting.native, NativeRequest::Plan) {
            turn::record_end(shared, TurnOutcome::Error, Some(error.to_string()), None, None).await;
            return Err(error);
        }
        shared
            .event(EventBody::Notice { level: NoticeLevel::Error, message: error.to_string(), subagent: None }, None)
            .await;
        if !refused {
            shared.stop().await;
        }
        turn::report_state(shared).await;
        return Err(error);
    }
    turn::report_state(shared).await;
    Ok(())
}

pub(super) fn can_answer(native: &NativeRequest, decision: &Decision) -> bool {
    matches!(
        (native, decision),
        (NativeRequest::Plan, Decision::ApprovePlan { context: PlanContext::Kept, .. } | Decision::KeepPlanning { .. })
            | (
                NativeRequest::Permission { .. },
                Decision::Allow { remember: Remember::Once | Remember::Conversation }
                    | Decision::Deny { words: None, keep: None, then: AfterDenial::Continue | AfterDenial::Stop }
            )
            | (NativeRequest::Elicitation { .. }, Decision::Answer { .. })
    )
}

async fn answer(shared: &Arc<Shared>, waiting: &Waiting, decision: &Decision) -> Result<(), AgentError> {
    match (&waiting.native, decision) {
        (NativeRequest::Plan, Decision::ApprovePlan { context: PlanContext::Kept, .. }) => {
            switch_plan_mode(shared, PermissionMode::FullAccess, "Implement the approved plan.").await
        }
        (NativeRequest::Plan, Decision::KeepPlanning { words }) => {
            let text =
                words.as_deref().filter(|w| !w.trim().is_empty()).unwrap_or("Keep planning; the plan is not approved.");
            switch_plan_mode(shared, PermissionMode::Plan, text).await
        }
        (NativeRequest::Permission { id, request }, Decision::Allow { remember }) => {
            let always = match remember {
                Remember::Once => false,
                Remember::Conversation => true,
                Remember::Rule(_) | Remember::Unknown => return Err(unsupported_decision()),
            };
            let option = request.option_for(always, true).unwrap_or(if always { "allow_always" } else { "allow" });
            shared.rpc().reply(id.clone(), acp::permission_selected(option)).await
        }
        (NativeRequest::Permission { id, request }, Decision::Deny { words: None, then, keep: None }) => {
            let call_id = ToolUseId::from(request.tool_call.tool_call_id.as_str());
            shared.state.lock().denied_calls.insert(call_id);
            let reject = request.option_for(false, false).unwrap_or("reject");
            shared.rpc().reply(id.clone(), acp::permission_selected(reject)).await?;
            if *then == AfterDenial::Stop {
                let session_id = {
                    let mut state = shared.state.lock();
                    let session_id = state.session_id.clone();
                    if session_id.is_some() {
                        state.cancelling = true;
                    }
                    session_id
                };
                if let Some(session_id) = session_id {
                    shared.rpc().notify("session/cancel", json!({ "sessionId": session_id })).await?;
                }
            }
            Ok(())
        }
        (NativeRequest::Elicitation { id, fields }, Decision::Answer { answers }) => {
            shared.rpc().reply(id.clone(), acp::elicitation_accept(fields, answers)).await
        }
        _ => Err(unsupported_decision()),
    }
}

async fn switch_plan_mode(shared: &Arc<Shared>, mode: PermissionMode, prompt_text: &str) -> Result<(), AgentError> {
    let changed = {
        let mut state = shared.state.lock();
        if state.session_id.is_none() {
            return Err(AgentError::Exited);
        }
        // The turn goes on in the prompt below, and keeps what the plan's prompt took.
        state.ending = Ending::Not;
        let changed = state.mode != mode;
        state.mode = mode;
        changed
    };
    if changed {
        shared.event(EventBody::AgentMode { permission_mode: mode }, None).await;
    }
    control::send_prompt_rpc(shared, None, vec![json!({ "type": "text", "text": prompt_text })]).await
}

fn unsupported_decision() -> AgentError {
    AgentError::Refused("Antigravity cannot apply that decision to this request".into())
}

async fn unsupported(shared: &Shared, id: &Value, method: &str) -> Result<(), AgentError> {
    shared
        .event(
            EventBody::Notice {
                level: NoticeLevel::Warning,
                message: format!("Antigravity asked for {method}, which this adapter cannot answer"),
                subagent: None,
            },
            None,
        )
        .await;
    shared.rpc().refuse(id.clone(), "Roundhouse does not support this request").await
}
