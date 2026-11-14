use std::collections::HashMap;

use protocol::capability::calls::{
    Call, CallOutput,
    call::{Runs, StartSubagent},
};
use protocol::event::{AgentTask, ContentBlock, EventBody, SubagentOutcome, TaskKind};
use protocol::model::ToolUseId;
use serde_json::Value;

use super::{
    native::{Activity, Kind},
    routing::Scope,
    tracking::{Child, Run},
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
            {
                let mut state = shared.state.lock();
                if state.subagents.threads.contains_key(&activity.agent_thread_id) {
                    return;
                }
                state
                    .subagents
                    .threads
                    .insert(activity.agent_thread_id, Child { current: call.clone(), turns: HashMap::new() });
                state.subagents.runs.insert(
                    call.clone(),
                    Run {
                        parent: scope.parent.clone(),
                        details: details.clone(),
                        turn: None,
                        reply: None,
                        ended: false,
                    },
                );
            }
            started(shared, call, scope.parent.clone(), details, Some(raw)).await;
        }
        // These describe an operation, not the lifetime of a child turn. In particular a delayed completion
        // activity cannot close a follow-up that has since started on the same thread.
        Kind::Interacted | Kind::Interrupted | Kind::Completed | Kind::Unknown => {}
    }
}

/// Bind the child turn before routing its notifications. A new turn on a finished child is a new invocation,
/// since the hub's subagent records are terminal once ended.
pub(in crate::codex) async fn turn_started(shared: &Shared, params: &Value) -> Result<(), AgentError> {
    let Some(thread) = params["threadId"].as_str() else { return Ok(()) };
    let thread = ThreadId(thread.into());
    let turn: TurnId = serde_json::from_value(params["turn"]["id"].clone())?;
    let start = {
        let mut state = shared.state.lock();
        let agents = &mut state.subagents;
        let Some(child) = agents.threads.get_mut(&thread) else { return Ok(()) };
        if child.turns.contains_key(&turn) {
            return Ok(());
        }
        let Some(previous) = agents.runs.get(&child.current) else { return Ok(()) };
        let start = if previous.turn.is_some() {
            let mut details = previous.details.clone();
            details.resumes = Some(child.current.clone());
            let call: ToolUseId = format!("codex-subagent-{}-{}", thread.0, turn.0).into();
            let parent = previous.parent.clone();
            agents.runs.insert(
                call.clone(),
                Run {
                    parent: parent.clone(),
                    details: details.clone(),
                    turn: Some(turn.clone()),
                    reply: None,
                    ended: false,
                },
            );
            child.current = call.clone();
            Some((call, parent, details))
        } else {
            if let Some(run) = agents.runs.get_mut(&child.current) {
                run.turn = Some(turn.clone());
            }
            None
        };
        child.turns.insert(turn, child.current.clone());
        start
    };
    if let Some((call, parent, details)) = start {
        started(shared, call, parent, details, None).await;
    }
    Ok(())
}

async fn started(
    shared: &Shared,
    call: ToolUseId,
    parent: Option<ToolUseId>,
    details: StartSubagent,
    raw: Option<String>,
) {
    shared
        .event(
            EventBody::Assistant {
                message_id: Some(call.to_string().into()),
                parent_tool_use_id: parent,
                model: None,
                blocks: vec![ContentBlock::Call { id: call.clone(), call: Call::Subagent(details.clone()) }],
                first_index: None,
            },
            raw,
        )
        .await;
    shared
        .event(
            EventBody::SubagentStarted {
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

pub(in crate::codex) async fn complete(shared: &Shared, call: &ToolUseId, turn: &Value, raw: String) {
    let ended = {
        let mut state = shared.state.lock();
        let Some(run) = state.subagents.runs.get_mut(call) else { return };
        if run.ended {
            return;
        }
        run.ended = true;
        (run.parent.clone(), run.reply.take())
    };
    control::settle(shared, Some(call)).await;
    let outcome = match turn["status"].as_str() {
        Some("completed") => SubagentOutcome::Finished,
        Some("interrupted") => SubagentOutcome::Stopped,
        Some("failed") => SubagentOutcome::Failed,
        _ => SubagentOutcome::Unknown,
    };
    let (parent, report) = ended;
    shared
        .event(
            EventBody::SubagentEnded { call: call.clone(), outcome, report: report.clone(), context_tokens: None },
            Some(raw),
        )
        .await;
    shared
        .event(
            EventBody::AgentUser {
                parent_tool_use_id: parent,
                blocks: vec![ContentBlock::CallResult {
                    tool_use_id: call.clone(),
                    output: CallOutput::Report { report: report.unwrap_or_default() },
                    is_error: matches!(outcome, SubagentOutcome::Failed),
                    denied: None,
                }],
            },
            None,
        )
        .await;
    tasks(shared).await;
}

pub(in crate::codex) async fn tasks(shared: &Shared) {
    let (mut tasks, idle) = {
        let state = shared.state.lock();
        let tasks: Vec<_> = state
            .subagents
            .runs
            .iter()
            .filter(|(_, run)| !run.ended)
            .map(|(call, run)| AgentTask {
                id: call.to_string().into(),
                description: run.details.description.clone(),
                kind: TaskKind::Subagent,
                call: Some(call.clone()),
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
