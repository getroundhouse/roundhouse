use protocol::capability::{
    approvals::{AfterDenial, Asks, Decision, Remember, Request},
    asking::{Choose, PlanContext, Question, QuestionOption, TypedAnswer},
};
use protocol::event::{AgentRunState, EventBody, NoticeLevel, TurnOutcome};
use protocol::model::{ApprovalId, ToolUseId};
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
    // A call's request names the call, as Claude's do, so a client ties the call's result to it.
    let (asks, native, call_id) = match method {
        "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
            let id: ToolUseId = params["itemId"].as_str().unwrap_or_default().into();
            let call = shared.state.lock().work(scope.parent.as_ref()).calls.get(&id).cloned();
            let Some(call) = call else { return unsupported(shared, native_id, method).await };
            let native = if method.contains("commandExecution") {
                NativeRequest::Command(native_id.clone())
            } else {
                NativeRequest::File(native_id.clone())
            };
            (Asks::Call { call }, native, Some(id))
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
            (Asks::Questions { questions }, NativeRequest::Questions { id: native_id.clone(), keys }, None)
        }
        _ => return unsupported(shared, native_id, method).await,
    };
    let request =
        Request { asks, suggested: Vec::new(), always: false, why: params["reason"].as_str().map(str::to_owned) };
    let id = ApprovalId::from(format!("codex-{}", uuid::Uuid::new_v4()));
    ask(shared, id, Waiting { request, native, parent: scope.parent.clone() }, call_id, Some(raw)).await;
    Ok(())
}

pub(super) async fn plan(shared: &Shared, plan: String) {
    let id = ApprovalId::from(format!("codex-plan-{}", uuid::Uuid::new_v4()));
    let request = Request { asks: Asks::Plan { plan, file: None }, suggested: Vec::new(), always: false, why: None };
    ask(shared, id, Waiting { request, native: NativeRequest::Plan, parent: None }, None, None).await;
}

/// Records the request `waiting`, as `id`, naming the call it asks to make, if it asks to make one, and waits on its
/// answer.
async fn ask(shared: &Shared, id: ApprovalId, waiting: Waiting, call: Option<ToolUseId>, raw: Option<String>) {
    let (request, parent) = (waiting.request.clone(), waiting.parent.clone());
    shared.state.lock().waiting.insert(id.clone(), waiting);
    shared
        .event(
            EventBody::ApprovalRequested {
                approval_id: id,
                request: Box::new(request),
                tool_use_id: call,
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
            .event(EventBody::Notice { level: NoticeLevel::Error, message: error.to_string(), subagent: None }, None)
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

pub(super) fn can_answer(native: &NativeRequest, decision: &Decision) -> bool {
    matches!(
        (native, decision),
        (NativeRequest::Plan, Decision::ApprovePlan { context: PlanContext::Kept, .. } | Decision::KeepPlanning { .. })
            | (
                NativeRequest::Command(_) | NativeRequest::File(_),
                Decision::Allow { remember: Remember::Once | Remember::Conversation }
                    | Decision::Deny { words: None, keep: None, then: AfterDenial::Continue | AfterDenial::Stop }
            )
            | (NativeRequest::Questions { .. }, Decision::Answer { .. })
    )
}

async fn answer(shared: &Shared, waiting: &Waiting, decision: &Decision) -> Result<(), AgentError> {
    match (&waiting.native, decision) {
        (NativeRequest::Plan, Decision::ApprovePlan { context: PlanContext::Kept, .. }) => {
            shared.state.lock().settings.mode = super::claims::full_access();
            shared.event(EventBody::AgentMode { mode: super::claims::full_access() }, None).await;
            control::turn(shared, vec![json!({"type":"text","text":"Implement the approved plan.","text_elements":[]})])
                .await
        }
        (NativeRequest::Plan, Decision::KeepPlanning { words }) => {
            shared.state.lock().settings.mode = super::claims::plan();
            shared.event(EventBody::AgentMode { mode: super::claims::plan() }, None).await;
            control::turn(shared, vec![json!({"type":"text","text":words.as_deref().unwrap_or("Keep planning; the plan is not approved."),"text_elements":[]})]).await
        }
        (NativeRequest::Command(id) | NativeRequest::File(id), Decision::Allow { remember }) => {
            let decision = match remember {
                Remember::Once => "accept",
                Remember::Conversation => "acceptForSession",
                Remember::Rule(_) | Remember::Unknown => return Err(unsupported_decision()),
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
fn words<'a>(question: &'a Question, answer: &'a protocol::capability::asking::QuestionAnswer) -> Vec<&'a str> {
    let picked = answer.picked.iter().filter_map(|&index| question.options.get(usize::try_from(index).ok()?));
    let picked = picked.take(if question.choose == Choose::Several { usize::MAX } else { 1 });
    let typed = answer.typed.as_deref().filter(|_| matches!(question.typed, TypedAnswer::Yes | TypedAnswer::Secret));
    picked.map(|option| option.label.as_str()).chain(typed).collect()
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
        shared.event(EventBody::ApprovalResolved { approval_id: id, decision: None, by_setting: false }, None).await;
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
                subagent: None,
            },
            None,
        )
        .await;
    shared.rpc.refuse(id.clone(), "Roundhouse does not support this request").await
}
