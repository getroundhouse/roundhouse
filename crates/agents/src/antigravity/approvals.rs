use protocol::capability::{
    approvals::{AfterDenial, Asks, Decision, Remember, Request},
    asking::PlanContext,
};
use protocol::event::{AgentRunState, ApprovalDecision, EventBody, NoticeLevel, TurnOutcome};
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
    shared: &Shared,
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
    let (mode, in_turn) = {
        let state = shared.state.lock();
        (state.mode, state.in_turn())
    };
    if !in_turn {
        let reject = perm.option_for(false, false).unwrap_or("reject");
        return shared.rpc.reply(native_id.clone(), acp::permission_selected(reject)).await;
    }
    if mode == PermissionMode::FullAccess {
        let allow = perm.option_for(false, true).unwrap_or("allow");
        return shared.rpc.reply(native_id.clone(), acp::permission_selected(allow)).await;
    }
    notifications::ensure_call_emitted(shared, &perm.tool_call, None).await;
    let id = ToolUseId::from(perm.tool_call.tool_call_id.as_str());
    let call = shared.state.lock().calls.get(&id).cloned().unwrap_or_else(|| calls::translate_call(&perm.tool_call));
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
    if !shared.state.lock().in_turn() {
        return shared.rpc.reply(native_id.clone(), acp::elicitation_cancel()).await;
    }
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
                request: Some(Box::new(request)),
                tool_name: String::new(),
                tool_use_id: call,
                input: Value::Null,
                description: None,
                suggestions: Box::new(Value::Null),
                asking: None,
                reason: None,
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
    for (id, payload) in replies {
        let _ = shared.rpc.reply(id, payload).await;
    }
}

pub(super) async fn decide(shared: &Shared, id: &ApprovalId, decision: &Decision) -> Result<(), AgentError> {
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
            .event(
                EventBody::Notice { level: NoticeLevel::Error, message: error.to_string(), parent_tool_use_id: None },
                None,
            )
            .await;
        if !refused {
            shared.stop().await;
        }
        control::report_state(shared).await;
        return Err(error);
    }
    control::report_state(shared).await;
    Ok(())
}

fn can_answer(native: &NativeRequest, decision: &Decision) -> bool {
    matches!(
        (native, decision),
        (NativeRequest::Plan, Decision::ApprovePlan { context: PlanContext::Kept, .. } | Decision::KeepPlanning { .. })
            | (
                NativeRequest::Permission { .. },
                Decision::Allow { edited: None, remember: Remember::Once | Remember::Conversation }
                    | Decision::Deny { words: None, keep: None, .. }
            )
            | (NativeRequest::Elicitation { .. }, Decision::Answer { .. })
    )
}

async fn answer(shared: &Shared, waiting: &Waiting, decision: &Decision) -> Result<(), AgentError> {
    match (&waiting.native, decision) {
        (NativeRequest::Plan, Decision::ApprovePlan { context: PlanContext::Kept, .. }) => {
            switch_plan_mode(shared, PermissionMode::FullAccess, "Implement the approved plan.").await
        }
        (NativeRequest::Plan, Decision::KeepPlanning { words }) => {
            let text = words.as_deref().unwrap_or("Keep planning; the plan is not approved.");
            switch_plan_mode(shared, PermissionMode::Plan, text).await
        }
        (NativeRequest::Permission { id, request }, Decision::Allow { edited: None, remember }) => {
            let always = match remember {
                Remember::Once => false,
                Remember::Conversation => true,
                Remember::Rule(_) => return Err(unsupported_decision()),
            };
            let option = request.option_for(always, true).unwrap_or(if always { "allow_always" } else { "allow" });
            shared.rpc.reply(id.clone(), acp::permission_selected(option)).await
        }
        (NativeRequest::Permission { id, request }, Decision::Deny { words: None, then, keep: None }) => {
            let call_id = ToolUseId::from(request.tool_call.tool_call_id.as_str());
            shared.state.lock().denied_calls.insert(call_id);
            let reject = request.option_for(false, false).unwrap_or("reject");
            shared.rpc.reply(id.clone(), acp::permission_selected(reject)).await?;
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
                    shared.rpc.notify("session/cancel", json!({ "sessionId": session_id })).await?;
                }
            }
            Ok(())
        }
        (NativeRequest::Elicitation { id, fields }, Decision::Answer { answers }) => {
            shared.rpc.reply(id.clone(), acp::elicitation_accept(fields, answers)).await
        }
        _ => Err(unsupported_decision()),
    }
}

async fn switch_plan_mode(shared: &Shared, mode: PermissionMode, prompt_text: &str) -> Result<(), AgentError> {
    let session_id = shared.state.lock().session_id.clone().ok_or(AgentError::Exited)?;
    shared
        .rpc
        .ask("session/set_mode", json!({ "sessionId": session_id, "modeId": super::running::mode_id(mode) }))
        .await?;
    let changed = {
        let mut state = shared.state.lock();
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

pub(super) fn legacy(shared: &Shared, id: &ApprovalId, decision: &ApprovalDecision) -> Result<Decision, AgentError> {
    let state = shared.state.lock();
    let waiting = state
        .waiting
        .get(id)
        .ok_or_else(|| AgentError::Refused("That Antigravity request is no longer open".into()))?;
    match decision {
        ApprovalDecision::Allow { updated_input: None, updated_permissions: None } => {
            Ok(if matches!(waiting.native, NativeRequest::Plan) {
                Decision::ApprovePlan { then: super::claims::full_access(), context: PlanContext::Kept }
            } else {
                Decision::Allow { edited: None, remember: Remember::Once }
            })
        }
        ApprovalDecision::Deny { message, interrupt: false } if matches!(waiting.native, NativeRequest::Plan) => {
            Ok(Decision::KeepPlanning { words: message.clone() })
        }
        ApprovalDecision::Deny { message, interrupt } => Ok(Decision::Deny {
            words: message.clone(),
            then: if *interrupt { AfterDenial::Stop } else { AfterDenial::Continue },
            keep: None,
        }),
        ApprovalDecision::AnswerByPosition { answers } => Ok(Decision::Answer { answers: answers.clone() }),
        ApprovalDecision::Allow { .. } | ApprovalDecision::AnswerByText { .. } => Err(unsupported_decision()),
    }
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
                parent_tool_use_id: None,
            },
            None,
        )
        .await;
    shared.rpc.refuse(id.clone(), "Roundhouse does not support this request").await
}
