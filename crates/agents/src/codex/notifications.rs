use std::sync::Arc;

use protocol::event::{AgentRunState, EventBody, NoticeLevel, TurnOutcome};
use protocol::tail::{DeltaChange, StreamDelta};
use serde_json::Value;
use tokio::sync::mpsc;

use super::state::{Ended, ThreadId, TurnId};
use super::{approvals, control, items, notices, running::Shared, subagents, turn_reports, usage};
use crate::{AgentError, AgentOutput};

pub(super) async fn read(shared: Arc<Shared>, mut input: mpsc::Receiver<(Value, String)>) {
    while let Some((value, raw)) = input.recv().await {
        if value.get("method").is_none() {
            shared.rpc.answered(&value);
            continue;
        }
        // Kept with the notice of a line that can't be read, as every other line is kept with its event, so what it
        // said can be read again once a newer Codex's shape is known.
        let line = raw.clone();
        if let Err(error) = notification(&shared, &value, raw).await {
            shared
                .run
                .event(
                    EventBody::Notice {
                        level: NoticeLevel::Error,
                        message: format!("Could not read Codex's message: {error}"),
                        subagent: subagents::scope(&shared, &value["params"]).and_then(|scope| scope.parent),
                    },
                    Some(line),
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
            .run
            .event(
                EventBody::AgentOther {
                    kind: format!("unassociated Codex thread: {method}"),
                    // Named by its thread, as a subagent is, so it's kept apart from the agent's own events.
                    subagent: params["threadId"].as_str().map(|thread| ThreadId(thread.into()).subagent()),
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

/// What Codex says that the adapter has no use for, which it asks Codex not to send as it initializes, rather than
/// dropping each as it comes.
pub(super) const UNUSED: [&str; 11] = [
    // Idle can precede turn/completed. Only the normalized outcome makes Roundhouse idle.
    "thread/status/changed",
    "thread/started",
    // What the finished item says whole: a command's output and a reasoning summary's parts, which would otherwise put
    // an event that draws nothing in the log for every chunk, thousands between a call and its result.
    "item/reasoning/textDelta",
    "item/commandExecution/outputDelta",
    "item/reasoning/summaryPartAdded",
    // The turn's diff, which its file changes and the worktree hold.
    "turn/diff/updated",
    // State Codex keeps for its own interfaces: which of a thread's turns its app has read, whether its remote
    // control is connected, and how its moderation classified the turn.
    "thread/readState/changed",
    "remoteControl/status/changed",
    "turn/moderationMetadata",
    // The thread's settings as it starts, which the adapter sets again with each turn it starts.
    "thread/settings/updated",
    // A compaction, as its `contextCompaction` item says too, which Codex's schema calls the one to read.
    "thread/compacted",
];

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
            let compacting = {
                let mut state = shared.state.lock();
                state.turn = Some(id.clone());
                state.reply = None;
                state.plan = None;
                // The turn Codex compacts in, which takes the compaction asked for, as the person's message takes a
                // prompt.
                state.compaction.as_mut().filter(|asked| asked.turn.is_none()).map(|asked| {
                    asked.turn = Some(id);
                    asked.asked
                })
            };
            shared.run.event(EventBody::AgentState { state: AgentRunState::Running }, Some(raw)).await;
            if let Some(prompt) = compacting {
                shared.run.event(EventBody::PromptTaken { prompt }, None).await;
            }
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
        "thread/tokenUsage/updated" => usage::tokens(shared, params, scope, raw).await,
        "account/rateLimits/updated" => usage::limits(shared, params, scope, raw).await,
        "warning" => notices::warning(shared, params, scope, raw).await,
        "mcpServer/startupStatus/updated" => notices::mcp_server(shared, params, scope, raw).await,
        "error" => notices::error(shared, params, scope, raw).await,
        "turn/plan/updated" => turn_reports::plan(shared, params, scope).await?,
        // From a Codex that sends them though it was asked not to.
        method if UNUSED.contains(&method) => {}
        // Codex waiting on a running command with nothing to write to it, which the finished command says whole.
        "item/commandExecution/terminalInteraction" if params["stdin"] == "" => {}
        _ => {
            shared
                .run
                .event(EventBody::AgentOther { kind: method.into(), subagent: scope.parent.clone() }, Some(raw))
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
        .run
        .emit(AgentOutput::Delta(StreamDelta {
            run: None,
            message_id: id.into(),
            subagent: scope.parent.clone(),
            change,
        }))
        .await;
}

async fn complete(shared: &Shared, turn: &Value, raw: String) -> Result<(), AgentError> {
    let id: TurnId = serde_json::from_value(turn["id"].clone())?;
    let outcome = match turn["status"].as_str() {
        Some("completed") => TurnOutcome::Success,
        Some("interrupted") => TurnOutcome::Cancelled,
        _ => TurnOutcome::Error,
    };
    let error = turn["error"]["message"].as_str().map(str::to_owned);
    let plan = {
        let mut state = shared.state.lock();
        if state.turn.as_ref() != Some(&id) {
            return Ok(());
        }
        state.turn = None;
        state.last_ended = Some(id.clone());
        if state.cancelling {
            // The cancel waiting for this turn records how it ended, since the turn can have finished before Codex
            // read the interrupt.
            state.held_end = Some(Ended { outcome, error, raw });
            drop(state);
            shared.turn_done.send_replace(Some(id));
            return Ok(());
        }
        state.take_plan()
    };
    if outcome == TurnOutcome::Success
        && let Some(plan) = plan
    {
        approvals::plan(shared, plan).await;
        shared.turn_done.send_replace(Some(id));
        return Ok(());
    }
    control::settle(shared, None).await;
    record_end(shared, outcome, error, Some(raw)).await;
    shared.turn_done.send_replace(Some(id));
    Ok(())
}

/// Records how the agent's own turn ended, with what it last replied, and what the agent's doing now. A cancelled turn
/// has no reply, since what it last said was cut off.
pub(super) async fn record_end(shared: &Shared, outcome: TurnOutcome, error: Option<String>, raw: Option<String>) {
    let (reply, compacted) = {
        let mut state = shared.state.lock();
        (state.reply.take(), state.compaction.take_if(|asked| asked.turn.is_some()))
    };
    // A turn that was only the compaction asked for, as Codex's is, whose end is drawn as the compaction is. One that
    // went wrong says so, and what's wrong, so its end is drawn.
    let only_compacted = outcome == TurnOutcome::Success && compacted.is_some_and(|asked| asked.answered);
    let end = EventBody::TurnEnded {
        outcome,
        result: reply.filter(|_| outcome != TurnOutcome::Cancelled),
        errors: error.into_iter().collect(),
        duration_ms: None,
        cost_usd: None,
        num_turns: None,
        started_at_ms: None,
        models: Vec::new(),
        tokens: None,
        only_compacted,
        request_failed: false,
    };
    shared.run.event(end, raw).await;
    control::report_state(shared).await;
}
