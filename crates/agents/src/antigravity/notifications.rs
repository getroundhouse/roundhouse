use std::sync::Arc;

use protocol::capability::Report;
use protocol::capability::calls::{Call, CallOutput, output::Ended};
use protocol::event::{AgentRunState, ContentBlock, Denied, EventBody, NoticeLevel, TaskOutcome};
use protocol::model::{ContextUsage, MaxSetBy, Seq, ToolUseId};
use protocol::tail::{BlockKind, DeltaChange, StreamDelta};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::{
    approvals, calls,
    running::Shared,
    state::{OpenCall, UserChunk},
    turn,
};
use crate::acp::{self, SessionUpdate, ToolCallWire, ToolStatusWire};
use crate::{AgentError, AgentOutput};

pub(super) async fn read(shared: Arc<Shared>, mut input: mpsc::Receiver<(Value, String)>) {
    while let Some((value, raw)) = input.recv().await {
        if value.get("method").is_none() {
            let rpc = shared.rpc();
            if let Some((id, reply)) = rpc.decode_reply(&value)
                && shared.state.lock().has_prompt_rpc(id)
            {
                turn::prompt_finished(&shared, id, reply, raw).await;
            } else {
                rpc.answered(&value);
            }
            continue;
        }
        let line = raw.clone();
        if let Err(error) = incoming(&shared, &value, raw).await {
            shared
                .event(
                    EventBody::Notice {
                        level: NoticeLevel::Error,
                        message: format!("Could not read Antigravity's message: {error}"),
                        subagent: None,
                    },
                    Some(line),
                )
                .await;
        }
    }
}

async fn incoming(shared: &Arc<Shared>, value: &Value, raw: String) -> Result<(), AgentError> {
    let method = value["method"].as_str().unwrap_or_default();
    let params = &value["params"];
    if value.get("id").is_some() {
        flush_streams(shared).await;
        return approvals::requested(shared, method, &value["id"], params, raw).await;
    }
    if method != "session/update" {
        shared.event(EventBody::AgentOther { kind: method.into(), subagent: None }, Some(raw)).await;
        return Ok(());
    }
    session_update(shared, acp::parse_session_update(params), raw).await;
    Ok(())
}

async fn session_update(shared: &Shared, update: SessionUpdate, raw: String) {
    match update {
        SessionUpdate::UserMessageChunk { .. } => on_user_message(shared, raw).await,
        SessionUpdate::AgentThoughtChunk { text } => {
            if !text.is_empty() && !shared.state.lock().is_compacting() {
                ensure_prompt_taken(shared).await;
                ensure_own_turn_started(shared).await;
                flush_text(shared).await;
                stream_chunk(shared, true, &text).await;
            }
        }
        SessionUpdate::AgentMessageChunk { text } => on_agent_message_chunk(shared, &text).await,
        SessionUpdate::ToolCall(wire) | SessionUpdate::ToolCallUpdate(wire) => {
            flush_streams(shared).await;
            on_tool(shared, wire, raw).await;
        }
        SessionUpdate::CurrentModeUpdate { mode_id } => on_mode(shared, &mode_id, raw).await,
        SessionUpdate::ConfigOptionUpdate { config_options } => {
            let mut state = shared.state.lock();
            let matches_model = config_options
                .iter()
                .find(|opt| opt.id == "model")
                .and_then(|opt| opt.current_value.as_deref())
                .is_none_or(|reported| reported == state.model.as_str());
            if matches_model {
                state.config_options = config_options;
            }
        }
        SessionUpdate::UsageUpdate { used, size } => {
            if shared.state.lock().is_compacting() {
                return;
            }
            shared
                .emit(AgentOutput::Report(Report::Context {
                    context: ContextUsage {
                        used_tokens: used,
                        max_tokens: size,
                        model_tokens: Some(size),
                        max_set_by: Some(MaxSetBy::Model),
                        parts: Vec::new(),
                    },
                }))
                .await;
        }
        SessionUpdate::Other => {}
    }
}

async fn on_agent_message_chunk(shared: &Shared, text: &str) {
    if text.is_empty() {
        return;
    }
    let compacting = {
        let mut state = shared.state.lock();
        let compacting = state.is_compacting();
        if compacting {
            state.reply.get_or_insert_default().push_str(text);
        }
        compacting
    };
    if !compacting {
        ensure_prompt_taken(shared).await;
        ensure_own_turn_started(shared).await;
        flush_thought(shared).await;
        stream_chunk(shared, false, text).await;
    }
}

pub(super) async fn ensure_prompt_taken(shared: &Shared) {
    let Some(taken) = shared.state.lock().take_implicit_prompt() else { return };
    finish_prompt_intake(shared, taken.0, None).await;
}

async fn finish_prompt_intake(shared: &Shared, prompt: Option<Seq>, raw: Option<String>) {
    flush_streams(shared).await;
    {
        let mut state = shared.state.lock();
        state.reply = None;
        state.ended_tasks.clear();
    }
    if let Some(prompt) = prompt {
        shared.event(EventBody::PromptTaken { prompt }, raw).await;
        shared.event(EventBody::AgentState { state: AgentRunState::Running }, None).await;
    }
}

pub(super) async fn ensure_own_turn_started(shared: &Shared) {
    let started = shared.state.lock().begin_own_turn();
    if let Some(why) = started {
        shared.event(EventBody::AgentState { state: AgentRunState::Running }, None).await;
        shared.event(EventBody::OwnTurn(why), None).await;
    }
}

async fn on_user_message(shared: &Shared, raw: String) {
    let chunk = shared.state.lock().accept_user_chunk();
    match chunk {
        UserChunk::Ignore => {}
        UserChunk::FirstOfPrompt(prompt) => finish_prompt_intake(shared, prompt, Some(raw)).await,
        UserChunk::ContinueDrain => {
            ensure_own_turn_started(shared).await;
            flush_streams(shared).await;
            shared.state.lock().reply = None;
        }
        UserChunk::CancelDrain { session_id } => {
            let has_ended = !shared.state.lock().ended_tasks.is_empty();
            if has_ended {
                ensure_own_turn_started(shared).await;
            }
            flush_streams(shared).await;
            let _ = shared.rpc().notify("session/cancel", json!({ "sessionId": session_id })).await;
        }
    }
}

async fn stream_chunk(shared: &Shared, thinking: bool, text: &str) {
    if text.is_empty() {
        return;
    }
    let (id, started) = {
        let mut state = shared.state.lock();
        let slot = if thinking { &mut state.streaming_thought } else { &mut state.streaming_text };
        if let Some((id, buf)) = slot {
            buf.push_str(text);
            (id.clone(), false)
        } else {
            let prefix = if thinking { "thought" } else { "msg" };
            let id = state.next_message_id(prefix);
            let slot = if thinking { &mut state.streaming_thought } else { &mut state.streaming_text };
            *slot = Some((id.clone(), text.to_owned()));
            (id, true)
        }
    };
    if started {
        let kind = if thinking { BlockKind::Thinking } else { BlockKind::Text };
        shared
            .emit(AgentOutput::Delta(StreamDelta {
                run: None,
                message_id: id.clone().into(),
                subagent: None,
                change: DeltaChange::BlockStart { index: 0, kind, tool_name: None },
            }))
            .await;
    }
    let change = if thinking {
        DeltaChange::Thinking { index: 0, text: text.to_owned() }
    } else {
        DeltaChange::Text { index: 0, text: text.to_owned() }
    };
    shared.emit(AgentOutput::Delta(StreamDelta { run: None, message_id: id.into(), subagent: None, change })).await;
}

pub(super) async fn flush_streams(shared: &Shared) {
    flush_thought(shared).await;
    flush_text(shared).await;
}

async fn flush_thought(shared: &Shared) {
    let flushed = {
        let mut state = shared.state.lock();
        state.streaming_thought.take().map(|(id, text)| (id, text, state.model.as_str().to_owned()))
    };
    if let Some((id, text, model)) = flushed {
        shared
            .event(
                EventBody::Assistant {
                    message_id: Some(id.into()),
                    subagent: None,
                    model: Some(model),
                    blocks: vec![ContentBlock::Thinking { text }],
                    first_index: Some(0),
                },
                None,
            )
            .await;
    }
}

async fn flush_text(shared: &Shared) {
    let flushed = {
        let mut state = shared.state.lock();
        let taken = state.streaming_text.take();
        if let Some((_, text)) = &taken {
            state.reply = Some(text.clone());
        }
        taken.map(|(id, text)| (id, text, state.model.as_str().to_owned()))
    };
    if let Some((id, text, model)) = flushed {
        shared
            .event(
                EventBody::Assistant {
                    message_id: Some(id.into()),
                    subagent: None,
                    model: Some(model),
                    blocks: vec![ContentBlock::Text { text, images: Vec::new() }],
                    first_index: Some(0),
                },
                None,
            )
            .await;
    }
}

/// `agy` keeps backgrounded `run_command`, `schedule` and `run_workflow` calls in `in_progress` across `session/prompt`
/// endings and settles them later under the same `toolCallId`, whereas `invoke_subagent` completes its own call on
/// spawn and all other tools are turn-local (`docs/antigravity-cli.md`).
fn outlives_turn(call: &Call, status: ToolStatusWire) -> bool {
    status == ToolStatusWire::InProgress && matches!(call, Call::Command(_) | Call::Schedule(_) | Call::Workflow(_))
}

pub(super) async fn ensure_call_emitted(shared: &Shared, wire: &ToolCallWire, raw: Option<String>) {
    let id = ToolUseId::from(wire.tool_call_id.as_str());
    let to_emit = {
        let mut state = shared.state.lock();
        if state.emitted_calls.insert(id.clone()) {
            state.reply = None;
            let call = calls::translate_call(wire);
            let order = state.next_order();
            let outlives = wire.status.is_some_and(|status| outlives_turn(&call, status));
            state.calls.insert(id.clone(), OpenCall { call: call.clone(), order, outlives_turn: outlives });
            Some((call, state.model.as_str().to_owned()))
        } else {
            if let Some(status) = wire.status
                && let Some(open) = state.calls.get_mut(&id)
            {
                open.outlives_turn = outlives_turn(&open.call, status);
            }
            None
        }
    };
    if let Some((call, model)) = to_emit {
        ensure_prompt_taken(shared).await;
        ensure_own_turn_started(shared).await;
        shared
            .event(
                EventBody::Assistant {
                    message_id: Some(format!("agy-call-{id}").into()),
                    subagent: None,
                    model: Some(model),
                    blocks: vec![ContentBlock::Call { id, call }],
                    first_index: None,
                },
                raw,
            )
            .await;
    }
}

async fn on_tool(shared: &Shared, wire: ToolCallWire, raw: String) {
    let completed = matches!(wire.status, Some(ToolStatusWire::Completed | ToolStatusWire::Failed));
    ensure_call_emitted(shared, &wire, (!completed).then(|| raw.clone())).await;
    if !completed {
        return;
    }
    let id = ToolUseId::from(wire.tool_call_id.as_str());
    let settled = {
        let mut state = shared.state.lock();
        let was_denied = state.denied_calls.remove(&id);
        state.calls.remove(&id).map(|open| {
            state.async_agents.on_tool_completed(&open.call, &wire);
            (open.call, state.cancelling, was_denied)
        })
    };
    let Some((call, cancelling, was_denied)) = settled else { return };
    let output = calls::translate_output(&call, &wire, cancelling, was_denied);
    let is_error = wire.status == Some(ToolStatusWire::Failed);
    let task_outcome = task_outcome_of(&output, is_error);
    let denied = (is_error && was_denied).then_some(Denied { said: None });
    shared
        .event(
            EventBody::AgentUser {
                subagent: None,
                blocks: vec![ContentBlock::CallResult {
                    tool_use_id: id.clone(),
                    output,
                    is_error,
                    denied,
                    images: Vec::new(),
                }],
            },
            Some(raw),
        )
        .await;
    let updated_tasks = shared.state.lock().finish_listed_task(&id, task_outcome);
    if let Some((tasks, stopped)) = updated_tasks {
        shared.event(EventBody::AgentTasks { tasks, stopped }, None).await;
    }
}

fn task_outcome_of(output: &CallOutput, is_error: bool) -> TaskOutcome {
    let CallOutput::Command(cmd) = output else {
        return if is_error { TaskOutcome::Failed } else { TaskOutcome::Finished };
    };
    if cmd.ended == Ended::Stopped {
        TaskOutcome::Stopped
    } else if is_error || cmd.exit_code.is_some_and(|code| code != 0) {
        TaskOutcome::Failed
    } else {
        TaskOutcome::Finished
    }
}

async fn on_mode(shared: &Shared, mode_id: &str, raw: String) {
    let mode = match mode_id {
        "plan" => super::claims::plan(),
        "accept-edits" | "yolo" => super::claims::full_access(),
        // One `agy` added since, which neither skips its permissions nor plans.
        _ => protocol::capability::approvals::ApprovalMode::UNKNOWN,
    };
    let changed = {
        let mut state = shared.state.lock();
        if mode == state.process_mode {
            false
        } else {
            state.process_mode = mode;
            let changed = state.mode != mode;
            state.mode = mode;
            changed
        }
    };
    if changed {
        shared.event(EventBody::AgentMode { mode }, Some(raw)).await;
    }
}
