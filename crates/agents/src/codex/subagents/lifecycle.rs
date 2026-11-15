use protocol::capability::calls::{
    Call, CallOutput,
    call::{Runs, StartSubagent},
};
use protocol::event::{AgentTask, ContentBlock, EventBody, TaskKind, TaskOutcome};
use protocol::model::{SubagentId, ToolUseId};
use serde_json::Value;

use super::{
    native::{Activity, Kind},
    routing::Scope,
    tracking::Run,
};
use crate::{
    AgentError,
    codex::{
        control,
        running::Shared,
        state::{ThreadId, TurnId},
    },
};

pub(in crate::codex) async fn activity(shared: &Shared, scope: &Scope, activity: Activity, raw: String) {
    match activity.kind {
        Kind::Started => {
            let call = activity.id;
            let details = StartSubagent {
                persona: None,
                description: Some(activity.agent_path),
                prompt: String::new(),
                model: activity.model,
                effort: activity.reasoning_effort,
                runs: Runs::Background,
                resumes: None,
            };
            let subagent = activity.agent_thread_id.subagent();
            {
                let mut state = shared.state.lock();
                if state.subagents.runs.contains_key(&subagent) {
                    return;
                }
                let run = Run {
                    call: call.clone(),
                    parent: scope.parent.clone(),
                    details: details.clone(),
                    turn: None,
                    reply: None,
                    ended: false,
                    answered: false,
                };
                state.subagents.runs.insert(subagent.clone(), run);
            }
            shared
                .event(
                    EventBody::Assistant {
                        message_id: Some(call.to_string().into()),
                        subagent: scope.parent.clone(),
                        model: None,
                        blocks: vec![ContentBlock::Call { id: call.clone(), call: Call::Subagent(details.clone()) }],
                        first_index: None,
                    },
                    Some(raw),
                )
                .await;
            started(shared, subagent, call, details).await;
        }
        // These describe an operation, not the lifetime of a child turn. In particular a delayed completion
        // activity cannot close a follow-up that has since started on the same thread.
        Kind::Interacted | Kind::Interrupted | Kind::Completed | Kind::Unknown => {}
    }
}

/// Binds the child's turn before routing its notifications. A new turn on a child that had finished is a follow-up,
/// which starts the same subagent again, under the call that first started it, so its record works again.
pub(in crate::codex) async fn turn_started(shared: &Shared, params: &Value) -> Result<(), AgentError> {
    let Some(thread) = params["threadId"].as_str() else { return Ok(()) };
    let subagent = ThreadId(thread.into()).subagent();
    let turn: TurnId = serde_json::from_value(params["turn"]["id"].clone())?;
    let again = {
        let mut state = shared.state.lock();
        let Some(run) = state.subagents.runs.get_mut(&subagent) else { return Ok(()) };
        if run.turn.as_ref() == Some(&turn) {
            return Ok(());
        }
        run.turn = Some(turn);
        let again = std::mem::replace(&mut run.ended, false);
        if again {
            run.reply = None;
        }
        again.then(|| (run.call.clone(), run.details.clone()))
    };
    if let Some((call, details)) = again {
        started(shared, subagent, call, details).await;
    }
    Ok(())
}

/// Says `subagent`, which `call` started, started, or started again for a follow-up, and what works in the background
/// now.
async fn started(shared: &Shared, subagent: SubagentId, call: ToolUseId, details: StartSubagent) {
    shared
        .event(
            EventBody::SubagentStarted {
                subagent: Some(subagent),
                call,
                kind: details.persona.map(|name| name.to_string()),
                description: details.description,
                prompt: (!details.prompt.is_empty()).then_some(details.prompt),
                in_background: true,
            },
            None,
        )
        .await;
    tasks(shared).await;
}

/// Ends `subagent` as the turn `turn` says, if it's the one it's taking. Only its first end answers its call, which
/// the chat draws it in: a follow-up's report is its record's.
pub(in crate::codex) async fn complete(shared: &Shared, subagent: &SubagentId, turn: &Value, raw: String) {
    let ended = {
        let mut state = shared.state.lock();
        let Some(run) = state.subagents.runs.get_mut(subagent) else { return };
        let ending: Option<TurnId> = serde_json::from_value(turn["id"].clone()).ok();
        let another = ending.is_some() && run.turn.is_some() && ending != run.turn;
        if run.ended || another {
            return;
        }
        run.ended = true;
        let answers = !std::mem::replace(&mut run.answered, true);
        (run.call.clone(), run.parent.clone(), run.reply.take(), answers)
    };
    control::settle(shared, Some(subagent)).await;
    let outcome = match turn["status"].as_str() {
        Some("completed") => TaskOutcome::Finished,
        Some("interrupted") => TaskOutcome::Stopped,
        Some("failed") => TaskOutcome::Failed,
        _ => TaskOutcome::Unknown,
    };
    let (call, parent, report, answers) = ended;
    shared
        .event(
            EventBody::SubagentEnded {
                subagent: subagent.clone(),
                outcome,
                report: report.clone(),
                context_tokens: None,
            },
            Some(raw),
        )
        .await;
    if answers {
        shared
            .event(
                EventBody::AgentUser {
                    subagent: parent,
                    blocks: vec![ContentBlock::CallResult {
                        tool_use_id: call,
                        output: CallOutput::Report { report: report.unwrap_or_default() },
                        is_error: matches!(outcome, TaskOutcome::Failed),
                        denied: None,
                        images: Vec::new(),
                    }],
                },
                None,
            )
            .await;
    }
    tasks(shared).await;
}

pub(in crate::codex) async fn tasks(shared: &Shared) {
    let (mut tasks, idle) = {
        let state = shared.state.lock();
        let tasks: Vec<_> = state
            .subagents
            .runs
            .values()
            .filter(|run| !run.ended)
            .map(|run| AgentTask {
                id: run.call.to_string().into(),
                description: run.details.description.clone(),
                kind: TaskKind::Subagent,
                call: Some(run.call.clone()),
            })
            .collect();
        let idle = state.turn.is_none();
        (tasks, idle)
    };
    tasks.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
    shared.event(EventBody::AgentTasks { tasks, stopped: Vec::new() }, None).await;
    if idle {
        control::report_state(shared).await;
    }
}
