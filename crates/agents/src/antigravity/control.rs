use std::sync::atomic::Ordering;
use std::time::Duration;

use async_trait::async_trait;
use protocol::attachment::AttachmentFile;
use protocol::capability::{Control, settings};
use protocol::event::{AgentRunState, ApprovalDecision, ApprovalOutcome, EventBody, NoticeLevel, TurnOutcome};
use protocol::model::{AgentSessionId, ApprovalId, ModelName, PermissionMode, Seq, SessionSettingsChange};
use protocol::prompt::Prompt;
use serde_json::{Value, json};

use super::{
    approvals, launch, notifications,
    running::{AntigravityProcess, Shared, effort_str, ensure_model_offers_effort, mode_id},
    state::{Conversation, Ending, InFlightPrompt},
    turn,
};
use crate::{AgentError, AgentProcess, acp};

const END_WAIT: Duration = Duration::from_secs(10);

#[async_trait]
impl AgentProcess for AntigravityProcess {
    fn capabilities(&self) -> protocol::capability::Capabilities {
        super::claims::claims()
    }
    fn pid(&self) -> u32 {
        self.shared.pid
    }
    fn is_gone(&self) -> bool {
        self.shared.gone.load(Ordering::SeqCst)
    }

    async fn control(&self, control: &Control) -> Result<(), AgentError> {
        if !control.allowed(&self.capabilities()) {
            return Err(AgentError::UnknownRequest("Antigravity does not support that control".into()));
        }
        let _ordered = self.shared.controls.lock().await;
        match control {
            Control::Prompt { seq, prompt, .. } => prompt_to(&self.shared, *seq, prompt).await,
            Control::Cancel => cancel(&self.shared).await,
            Control::Decide { approval, decision } => approvals::decide(&self.shared, approval, decision).await,
            Control::Change(settings) => change(&self.shared, settings).await,
            Control::Note { .. }
            | Control::Aside { .. }
            | Control::Overrule { .. }
            | Control::Compact { .. }
            | Control::Rewind { .. }
            | Control::UndoRewind
            | Control::RestoreFiles { .. }
            | Control::PreviewRestoreFiles { .. }
            | Control::RunCommand { .. }
            | Control::StopTask { .. }
            | Control::ReadTaskOutput { .. }
            | Control::Reconnect { .. }
            | Control::SignInToServer { .. }
            | Control::SetGoal { .. }
            | Control::PauseGoal
            | Control::ClearGoal
            | Control::Review { .. }
            | Control::ReadRules
            | Control::ListModels
            | Control::ListOutside => {
                Err(AgentError::UnknownRequest("Antigravity does not support that control".into()))
            }
        }
    }

    async fn send_prompt(&self, seq: Seq, prompt: &Prompt<AttachmentFile>) -> Result<(), AgentError> {
        self.control(&Control::Prompt {
            seq,
            prompt: prompt.clone(),
            extras: protocol::capability::prompts::Extras::default(),
        })
        .await
    }
    async fn resolve_approval(&self, id: &ApprovalId, decision: &ApprovalDecision) -> Result<(), AgentError> {
        let decision = approvals::legacy(&self.shared, id, decision)?;
        self.control(&Control::Decide { approval: id.clone(), decision }).await
    }
    async fn interrupt(&self) -> Result<(), AgentError> {
        self.control(&Control::Cancel).await
    }
    async fn compact(&self, _asked: Seq, _keeping: Option<&str>) -> Result<(), AgentError> {
        Err(AgentError::UnknownRequest("Antigravity compaction is not offered by this adapter".into()))
    }
    async fn configure(&self, options: &SessionSettingsChange) -> Result<(), AgentError> {
        let settings =
            options.for_capabilities(&self.capabilities()).map_err(|error| AgentError::Refused(error.to_string()))?;
        self.control(&Control::Change(settings)).await
    }
    async fn stop(&self) {
        self.shared.stop().await;
    }
}

async fn prompt_to(shared: &Shared, seq: Seq, prompt: &Prompt<AttachmentFile>) -> Result<(), AgentError> {
    let blocks = if prompt.files.is_empty() {
        vec![json!({ "type": "text", "text": prompt.text })]
    } else {
        let owned = prompt.clone();
        tokio::task::spawn_blocking(move || acp::prompt_blocks(&owned)).await.map_err(|_| AgentError::Exited)?
    };
    send_prompt_rpc(shared, Some(seq), blocks).await
}

pub(super) async fn send_prompt_rpc(shared: &Shared, seq: Option<Seq>, blocks: Vec<Value>) -> Result<(), AgentError> {
    let user_chunks = blocks
        .iter()
        .filter(|block| {
            block.get("type").and_then(Value::as_str) == Some("text")
                && block.get("text").and_then(Value::as_str).is_some_and(|text| !text.is_empty())
        })
        .count();
    let ((session_id, rpc_id), _) = between_endings(shared, |state| {
        if seq.is_some() && !state.waiting.is_empty() {
            return Err(AgentError::Refused("Answer Antigravity's open request before sending another prompt".into()));
        }
        let session_id = state.session_id.clone().ok_or(AgentError::Exited)?;
        if state.prompts.is_empty() && state.ending != Ending::HeldForPlan {
            state.reply = None;
            state.cancelling = false;
        }
        let rpc_id = shared.rpc.next_id();
        state.prompts.push_back(InFlightPrompt { rpc_id, seq, taken: false, user_chunks_left: user_chunks });
        Ok((session_id, rpc_id))
    })
    .await?;
    let params = json!({ "sessionId": session_id, "prompt": blocks });
    if let Err(error) = shared.rpc.write_request(rpc_id, "session/prompt", params).await {
        // Some of it may have reached `agy`, so it's stopped rather than left in a state nobody knows, and its exit
        // drops the prompt, as it does any prompt `agy` hadn't taken.
        shared
            .event(EventBody::Notice { level: NoticeLevel::Error, message: error.to_string(), subagent: None }, None)
            .await;
        shared.stop().await;
    }
    Ok(())
}

/// Runs `act` on the conversation once no turn is part way through being recorded as ended, under the lock that saw
/// none was, so a turn's last prompt can't finish between the check and `act`. Returns what `act` returned, and
/// `Shared::turn_done` as it was then, for a wait on the next end. A turn still ending after `END_WAIT` stops `agy`.
async fn between_endings<T>(
    shared: &Shared,
    mut act: impl FnMut(&mut Conversation) -> Result<T, AgentError>,
) -> Result<(T, u64), AgentError> {
    loop {
        let mut done = shared.turn_done.subscribe();
        let ended = {
            let mut state = shared.state.lock();
            // Read under the lock, since the count changes only under it, with `ending`.
            let ended = *done.borrow_and_update();
            if state.ending != Ending::Recording {
                return act(&mut state).map(|acted| (acted, ended));
            }
            ended
        };
        if tokio::time::timeout(END_WAIT, done.wait_for(|count| *count > ended)).await.is_err() {
            shared.stop().await;
            return Err(AgentError::NoAnswer);
        }
    }
}

async fn change(shared: &Shared, requested: &settings::Settings) -> Result<(), AgentError> {
    let (session_id, current_model, current_effort) = {
        let state = shared.state.lock();
        let id = state.session_id.clone().ok_or(AgentError::Exited)?;
        (id, state.model.clone(), state.effort)
    };
    let next_model = requested.model.as_ref().unwrap_or(&current_model);
    let next_effort = requested.effort.unwrap_or(current_effort);
    let level = effort_str(next_effort)?;
    ensure_model_offers_effort(next_model, next_effort)?;
    let next_perm = match requested.mode {
        None => None,
        Some(mode) if mode == super::claims::plan() => Some(PermissionMode::Plan),
        Some(mode) if mode == super::claims::full_access() => Some(PermissionMode::FullAccess),
        Some(_) => return Err(AgentError::UnknownMode),
    };
    let mut switched_model = false;
    if let Some(model) = &requested.model {
        let reply = shared
            .rpc
            .ask(
                "session/set_config_option",
                json!({ "sessionId": session_id, "configId": "model", "value": model.as_str() }),
            )
            .await?;
        launch::update_options(shared, &reply);
        shared.state.lock().model.clone_from(model);
        switched_model = true;
    }
    let mut switched_effort = false;
    if requested.effort.is_some() || requested.model.is_some() {
        if let Err(error) = apply_effort(shared, &session_id, next_effort, level, requested.effort.is_some()).await {
            rollback_config(shared, &session_id, switched_model.then_some(&current_model), None).await;
            return Err(error);
        }
        switched_effort = true;
    }
    if let Some(perm) = next_perm
        && let Err(error) = set_mode(shared, &session_id, perm).await
    {
        rollback_config(
            shared,
            &session_id,
            switched_model.then_some(&current_model),
            switched_effort.then_some(current_effort),
        )
        .await;
        return Err(error);
    }
    Ok(())
}

async fn set_mode(shared: &Shared, session_id: &AgentSessionId, perm: PermissionMode) -> Result<(), AgentError> {
    shared.rpc.ask("session/set_mode", json!({ "sessionId": session_id, "modeId": mode_id(perm) })).await?;
    let changed = {
        let mut state = shared.state.lock();
        let changed = state.mode != perm;
        state.mode = perm;
        changed
    };
    if changed {
        shared.event(EventBody::AgentMode { permission_mode: perm }, None).await;
    }
    Ok(())
}

async fn rollback_config(
    shared: &Shared,
    session_id: &AgentSessionId,
    model: Option<&ModelName>,
    effort: Option<settings::Effort>,
) {
    if let Some(model) = model {
        if let Ok(reply) = shared
            .rpc
            .ask(
                "session/set_config_option",
                json!({ "sessionId": session_id, "configId": "model", "value": model.as_str() }),
            )
            .await
        {
            launch::update_options(shared, &reply);
        }
        shared.state.lock().model.clone_from(model);
    }
    if let Some(effort) = effort {
        if let Ok(level) = effort_str(effort) {
            let _ = apply_effort(shared, session_id, effort, level, false).await;
        }
        shared.state.lock().effort = effort;
    }
}

async fn apply_effort(
    shared: &Shared,
    session_id: &AgentSessionId,
    effort: settings::Effort,
    level: &str,
    explicit: bool,
) -> Result<(), AgentError> {
    let (has_thought, offers) = {
        let state = shared.state.lock();
        (
            state.config_options.iter().any(|opt| opt.id == "thought_level"),
            acp::config_offers_value(&state.config_options, "thought_level", level),
        )
    };
    if offers {
        let reply = shared
            .rpc
            .ask(
                "session/set_config_option",
                json!({ "sessionId": session_id, "configId": "thought_level", "value": level }),
            )
            .await?;
        launch::update_options(shared, &reply);
        shared.state.lock().effort = effort;
        Ok(())
    } else if explicit && has_thought {
        Err(AgentError::Refused("This Antigravity model does not offer that effort".into()))
    } else {
        shared.state.lock().effort = effort;
        Ok(())
    }
}

async fn cancel(shared: &Shared) -> Result<(), AgentError> {
    let (open, before) = between_endings(shared, |state| {
        if state.prompts.is_empty() && state.ending == Ending::Not && state.waiting.is_empty() {
            return Ok(None);
        }
        state.cancelling = !state.prompts.is_empty();
        Ok(Some((state.session_id.clone(), !state.prompts.is_empty())))
    })
    .await?;
    let Some((session_id, has_prompts)) = open else { return Ok(()) };
    // Waits on a count, and the turn may have ended already, which `wait_for` sees in the count before it waits.
    let mut done = shared.turn_done.subscribe();
    approvals::cancel_native_waiters(shared).await;
    if !has_prompts {
        notifications::flush_streams(shared).await;
        settle_turn(shared).await;
        turn::record_end(shared, TurnOutcome::Cancelled, None, None, None).await;
        return Ok(());
    }
    if let Some(session_id) = session_id
        && let Err(error) = shared.rpc.notify("session/cancel", json!({ "sessionId": session_id })).await
    {
        shared.stop().await;
        return Err(error);
    }
    if tokio::time::timeout(END_WAIT, done.wait_for(|count| *count > before)).await.is_err() {
        shared.stop().await;
        return Err(AgentError::NoAnswer);
    }
    Ok(())
}

pub(super) async fn settle_all(shared: &Shared) {
    let prompts: Vec<Seq> = {
        let mut state = shared.state.lock();
        for open in state.calls.values_mut() {
            open.outlives_turn = false;
        }
        state.prompts.drain(..).filter(|entry| !entry.taken).filter_map(|entry| entry.seq).collect()
    };
    for prompt in prompts {
        shared.event(EventBody::PromptDropped { prompt }, None).await;
    }
    settle_turn(shared).await;
}

pub(super) async fn settle_turn(shared: &Shared) {
    let (requests, calls) = {
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
        (requests, calls)
    };
    for approval_id in requests {
        shared
            .event(
                EventBody::ApprovalResolved {
                    approval_id,
                    outcome: ApprovalOutcome::Cancelled,
                    answers_by_text: None,
                    answers_by_position: None,
                    by_setting: false,
                },
                None,
            )
            .await;
    }
    for tool_use_id in calls {
        shared.event(EventBody::ToolCallDropped { tool_use_id }, None).await;
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
    shared.event(EventBody::AgentState { state }, None).await;
}
