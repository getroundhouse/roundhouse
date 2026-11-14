use protocol::capability::{
    approvals::{AfterDenial, Asks, Decision, Remember, Request},
    asking::{Choose, PlanContext, Question, QuestionOption, TypedAnswer},
};
use protocol::event::{AgentRunState, ApprovalDecision, ApprovalOutcome, EventBody, NoticeLevel, TurnOutcome};
use protocol::model::{ApprovalId, PermissionMode, ToolUseId};
use serde_json::{Value, json};

use super::{
    control, notifications,
    running::Shared,
    state::{NativeRequest, Waiting},
};
use crate::AgentError;

pub(super) async fn requested(
    shared: &Shared,
    method: &str,
    native_id: &Value,
    params: &Value,
    scope: &super::subagents::Scope,
    raw: String,
) -> Result<(), AgentError> {
    let (asks, native) = match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            let id = params["itemId"].as_str().unwrap_or_default();
            let call = shared.state.lock().work(scope.parent.as_ref()).calls.get(&id.into()).cloned();
            let Some(call) = call else { return unsupported(shared, native_id, method).await };
            let native = if method.contains("commandExecution") {
                NativeRequest::Command(native_id.clone())
            } else {
                NativeRequest::File(native_id.clone())
            };
            (Asks::Call { call }, native)
        }
        "item/tool/requestUserInput" => {
            let mut keys = Vec::new();
            let mut questions = Vec::new();
            for question in params["questions"].as_array().into_iter().flatten() {
                keys.push(question["id"].as_str().unwrap_or_default().to_owned());
                questions.push(Question {
                    text: question["question"].as_str().unwrap_or_default().into(),
                    header: question["header"].as_str().map(str::to_owned),
                    options: question["options"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|option| QuestionOption {
                            label: option["label"].as_str().unwrap_or_default().into(),
                            description: option["description"].as_str().map(str::to_owned),
                            preview: None,
                        })
                        .collect(),
                    choose: Choose::One,
                    typed: typed_answer(question),
                });
            }
            (Asks::Questions { questions }, NativeRequest::Questions { id: native_id.clone(), keys })
        }
        _ => return unsupported(shared, native_id, method).await,
    };
    let request =
        Request { asks, suggested: Vec::new(), always: false, why: params["reason"].as_str().map(str::to_owned) };
    ask(
        shared,
        ApprovalId::from(format!("codex-{}", uuid::Uuid::new_v4())),
        request,
        native,
        scope.parent.clone(),
        Some(raw),
    )
    .await;
    Ok(())
}

pub(super) async fn plan(shared: &Shared, plan: String) {
    let id = ApprovalId::from(format!("codex-plan-{}", uuid::Uuid::new_v4()));
    let request = Request { asks: Asks::Plan { plan, file: None }, suggested: Vec::new(), always: false, why: None };
    ask(shared, id, request, NativeRequest::Plan, None, None).await;
}

async fn ask(
    shared: &Shared,
    id: ApprovalId,
    request: Request,
    native: NativeRequest,
    parent: Option<ToolUseId>,
    raw: Option<String>,
) {
    shared
        .state
        .lock()
        .waiting
        .insert(id.clone(), Waiting { request: request.clone(), native, parent: parent.clone() });
    shared
        .event(
            EventBody::ApprovalRequested {
                approval_id: id,
                request: Some(Box::new(request)),
                tool_name: String::new(),
                tool_use_id: None,
                input: Value::Null,
                description: None,
                suggestions: Box::new(Value::Null),
                asking: None,
                reason: None,
                asked_by: parent,
            },
            raw,
        )
        .await;
    shared.event(EventBody::AgentState { state: AgentRunState::RequiresAction }, None).await;
}

/// Passes the person's decision on to Codex. The hub has recorded it already, with any secret answer taken out, so
/// nothing of it is recorded here.
pub(super) async fn decide(shared: &Shared, id: &ApprovalId, decision: &Decision) -> Result<(), AgentError> {
    let waiting = shared
        .state
        .lock()
        .waiting
        .remove(id)
        .ok_or_else(|| AgentError::Refused("That Codex request is no longer open".into()))?;
    if !can_answer(&waiting.native, decision) {
        shared.state.lock().waiting.insert(id.clone(), waiting);
        return Err(unsupported_decision());
    }
    if let Err(error) = answer(shared, &waiting, decision).await {
        // Codex refusing what answering asked of it, such as the turn after a plan, proves it did nothing. Anything
        // else may have reached it, so the process is stopped rather than left in a state nobody knows.
        let refused = super::rpc::refused(&error);
        if refused && matches!(waiting.native, NativeRequest::Plan) {
            // The plan's turn was held open for its decision, and nothing will carry it on, so it ends here.
            notifications::record_end(shared, TurnOutcome::Error, Some(error.to_string()), None).await;
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
                NativeRequest::Command(_) | NativeRequest::File(_),
                Decision::Allow { edited: None, remember: Remember::Once | Remember::Conversation }
                    | Decision::Deny { words: None, keep: None, .. }
            )
            | (NativeRequest::Questions { .. }, Decision::Answer { .. })
    )
}

async fn answer(shared: &Shared, waiting: &Waiting, decision: &Decision) -> Result<(), AgentError> {
    match (&waiting.native, decision) {
        (NativeRequest::Plan, Decision::ApprovePlan { context: PlanContext::Kept, .. }) => {
            shared.state.lock().settings.permission_mode = PermissionMode::FullAccess;
            shared.event(EventBody::AgentMode { permission_mode: PermissionMode::FullAccess }, None).await;
            control::turn(shared, vec![json!({"type":"text","text":"Implement the approved plan.","text_elements":[]})])
                .await
        }
        (NativeRequest::Plan, Decision::KeepPlanning { words }) => {
            shared.state.lock().settings.permission_mode = PermissionMode::Plan;
            shared.event(EventBody::AgentMode { permission_mode: PermissionMode::Plan }, None).await;
            control::turn(shared, vec![json!({"type":"text","text":words.as_deref().unwrap_or("Keep planning; the plan is not approved."),"text_elements":[]})]).await
        }
        (NativeRequest::Command(id) | NativeRequest::File(id), Decision::Allow { edited: None, remember }) => {
            let decision = match remember {
                Remember::Once => "accept",
                Remember::Conversation => "acceptForSession",
                Remember::Rule(_) => return Err(unsupported_decision()),
            };
            shared.rpc.write(&json!({"id":id,"result":{"decision":decision}})).await
        }
        (NativeRequest::Command(id) | NativeRequest::File(id), Decision::Deny { words: None, then, keep: None }) => {
            shared
                .rpc
                .write(
                    &json!({"id":id,"result":{"decision":if *then == AfterDenial::Stop {"cancel"} else {"decline"}}}),
                )
                .await
        }
        (NativeRequest::Questions { id, keys }, Decision::Answer { answers }) => {
            let Asks::Questions { questions } = &waiting.request.asks else { return Err(unsupported_decision()) };
            let mut result = serde_json::Map::new();
            for (i, (key, question)) in keys.iter().zip(questions).enumerate() {
                let words = answers.get(i).map_or_default(|answer| words(question, answer));
                result.insert(key.clone(), json!({"answers":words}));
            }
            shared.rpc.write(&json!({"id":id,"result":{"answers":result}})).await
        }
        _ => Err(unsupported_decision()),
    }
}

/// What Codex is told was answered to `question`: the options picked, as many as it lets be picked, and what was typed
/// where it lets an answer be typed, so nothing it ruled out reaches it.
fn words<'a>(question: &'a Question, answer: &'a protocol::event::QuestionAnswer) -> Vec<&'a str> {
    let picked = answer.picked.iter().filter_map(|&index| question.options.get(usize::try_from(index).ok()?));
    let picked = picked.take(if question.choose == Choose::Several { usize::MAX } else { 1 });
    let typed = answer.typed.as_deref().filter(|_| matches!(question.typed, TypedAnswer::Yes | TypedAnswer::Secret));
    picked.map(|option| option.label.as_str()).chain(typed).collect()
}

pub(super) fn legacy(shared: &Shared, id: &ApprovalId, decision: &ApprovalDecision) -> Result<Decision, AgentError> {
    let state = shared.state.lock();
    let waiting =
        state.waiting.get(id).ok_or_else(|| AgentError::Refused("That Codex request is no longer open".into()))?;
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

pub(super) async fn resolved(shared: &Shared, native: &Value) {
    let id = {
        let mut state = shared.state.lock();
        let id =
            state.waiting.iter().find_map(|(id, waiting)| (waiting.native.id() == Some(native)).then(|| id.clone()));
        if let Some(id) = &id {
            state.waiting.remove(id);
        }
        id
    };
    if let Some(id) = id {
        shared
            .event(
                EventBody::ApprovalResolved {
                    approval_id: id,
                    outcome: ApprovalOutcome::Cancelled,
                    answers_by_text: None,
                    answers_by_position: None,
                    by_setting: false,
                },
                None,
            )
            .await;
        control::report_state(shared).await;
    }
}

pub(super) fn typed_answer(question: &Value) -> TypedAnswer {
    if question["isSecret"] == true {
        TypedAnswer::Secret
    } else if question["isOther"] == true || question["options"].as_array().is_none_or(Vec::is_empty) {
        TypedAnswer::Yes
    } else {
        TypedAnswer::No
    }
}
fn unsupported_decision() -> AgentError {
    AgentError::Refused("Codex cannot apply that decision to this request".into())
}

pub(super) async fn unsupported(shared: &Shared, id: &Value, method: &str) -> Result<(), AgentError> {
    shared
        .event(
            EventBody::Notice {
                level: NoticeLevel::Warning,
                message: format!("Codex asked for {method}, which this adapter cannot answer"),
                parent_tool_use_id: None,
            },
            None,
        )
        .await;
    shared
        .rpc
        .write(&json!({"id":id,"error":{"code":-32601,"message":"Roundhouse does not support this request"}}))
        .await
}
