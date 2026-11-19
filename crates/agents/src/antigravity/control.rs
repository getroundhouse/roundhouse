use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use async_trait::async_trait;
use protocol::attachment::AttachmentFile;
use protocol::capability::approvals::ApprovalMode;
use protocol::capability::{Control, settings};
use protocol::event::{EventBody, NoticeLevel, TurnOutcome};
use protocol::model::{AgentSessionId, ModelName, Seq};
use protocol::prompt::Prompt;
use serde_json::{Value, json};

use super::{
    approvals, launch, notifications,
    running::{AntigravityProcess, Shared, effort_str, ensure_model_offers_effort, ensure_sign_in_offers_model},
    state::{Conversation, Ending, InFlightPrompt, PromptOrigin},
    swap::ensure_mode_process,
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
        self.shared.pid.load(Ordering::SeqCst)
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
            // Antigravity reads nothing to keep, which its claims say, so none is sent.
            Control::Compact { asked, .. } => super::compact::request(&self.shared, *asked).await,
            Control::Note { .. }
            | Control::Aside { .. }
            | Control::Overrule { .. }
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

    async fn stop(&self) {
        self.shared.stop().await;
    }
}

async fn prompt_to(shared: &Arc<Shared>, seq: Seq, prompt: &Prompt<AttachmentFile>) -> Result<(), AgentError> {
    let blocks = if prompt.files.is_empty() {
        vec![json!({ "type": "text", "text": prompt.text })]
    } else {
        let owned = prompt.clone();
        tokio::task::spawn_blocking(move || acp::prompt_blocks(&owned)).await.map_err(|_| AgentError::Exited)?
    };
    send_prompt_rpc(shared, Some(seq), blocks).await
}

fn ensure_prompt_ready(state: &Conversation, seq: Option<Seq>) -> Result<(), AgentError> {
    if state.is_compacting() {
        return Err(AgentError::Refused(
            "Wait for Antigravity to finish compacting before sending another prompt".into(),
        ));
    }
    if seq.is_some() && !state.waiting.is_empty() {
        return Err(AgentError::Refused("Answer Antigravity's open request before sending another prompt".into()));
    }
    Ok(())
}

pub(super) async fn send_prompt_rpc(
    shared: &Arc<Shared>,
    seq: Option<Seq>,
    blocks: Vec<Value>,
) -> Result<(), AgentError> {
    between_endings(shared, |state| ensure_prompt_ready(state, seq)).await?;
    ensure_mode_process(shared).await?;
    let user_chunks = non_empty_text_chunks(&blocks);
    let rpc = shared.rpc();
    let ((session_id, rpc_id), _) = between_endings(shared, |state| {
        ensure_prompt_ready(state, seq)?;
        let session_id = state.session_id.clone().ok_or(AgentError::Exited)?;
        if state.prompts.is_empty() && state.ending != Ending::HeldForPlan {
            state.reply = None;
            state.cancelling = false;
        }
        let rpc_id = rpc.next_id();
        state.prompts.push_back(InFlightPrompt {
            rpc_id,
            origin: PromptOrigin::Prompt(seq),
            taken: false,
            user_chunks_left: user_chunks,
        });
        Ok((session_id, rpc_id))
    })
    .await?;
    write_prompt_request(shared, &rpc, rpc_id, &session_id, blocks).await;
    Ok(())
}

pub(super) async fn send_drain_prompt(
    shared: &Shared,
    origin: PromptOrigin,
    blocks: Vec<Value>,
) -> Result<bool, AgentError> {
    let user_chunks = non_empty_text_chunks(&blocks);
    let rpc = shared.rpc();
    let (queued, _) = between_endings(shared, |state| {
        if state.in_turn() || !state.waiting.is_empty() || state.needs_mode_swap() {
            return Ok(None);
        }
        let session_id = state.session_id.clone().ok_or(AgentError::Exited)?;
        state.reply = None;
        state.cancelling = false;
        let rpc_id = rpc.next_id();
        state.prompts.push_back(InFlightPrompt { rpc_id, origin, taken: false, user_chunks_left: user_chunks });
        Ok(Some((session_id, rpc_id)))
    })
    .await?;
    let Some((session_id, rpc_id)) = queued else { return Ok(false) };
    write_prompt_request(shared, &rpc, rpc_id, &session_id, blocks).await;
    Ok(true)
}

fn non_empty_text_chunks(blocks: &[Value]) -> usize {
    blocks
        .iter()
        .filter(|block| {
            block.get("type").and_then(Value::as_str) == Some("text")
                && block.get("text").and_then(Value::as_str).is_some_and(|text| !text.is_empty())
        })
        .count()
}

pub(super) async fn write_prompt_request(
    shared: &Shared,
    rpc: &crate::jsonrpc::Rpc,
    rpc_id: u64,
    session_id: &AgentSessionId,
    blocks: Vec<Value>,
) {
    let params = json!({ "sessionId": session_id, "prompt": blocks });
    if let Err(error) = rpc.write_request(rpc_id, "session/prompt", params).await {
        // Some of it may have reached `agy`, so it's stopped rather than left in a state nobody knows, and its exit
        // drops the prompt, as it does any prompt `agy` hadn't taken.
        shared
            .event(EventBody::Notice { level: NoticeLevel::Error, message: error.to_string(), subagent: None }, None)
            .await;
        shared.stop().await;
    }
}

/// Runs `act` on the conversation once no turn is part way through being recorded as ended, under the lock that saw
/// none was, so a turn's last prompt can't finish between the check and `act`. Returns what `act` returned, and
/// `Shared::turn_done` as it was then, for a wait on the next end. A turn still ending after `END_WAIT` stops `agy`.
pub(super) async fn between_endings<T>(
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

async fn change(shared: &Arc<Shared>, requested: &settings::Settings) -> Result<(), AgentError> {
    let (session_id, current_model, current_effort, idle_for_swap, process_mode) = {
        let state = shared.state.lock();
        let id = state.session_id.clone().ok_or(AgentError::Exited)?;
        let idle = !state.in_turn() && state.waiting.is_empty();
        (id, state.model.clone(), state.effort.clone(), idle, state.process_mode)
    };
    let next_model = requested.model.as_ref().unwrap_or(&current_model);
    let next_effort = requested.effort.clone().unwrap_or(current_effort);
    let level = effort_str(&next_effort)?;
    ensure_model_offers_effort(next_model, &next_effort)?;
    if let Some(model) = &requested.model {
        ensure_sign_in_offers_model(&shared.state.lock().config_options, model)?;
    }
    let next_perm = match requested.mode {
        None => None,
        Some(mode) if mode == super::claims::plan() || mode == super::claims::full_access() => Some(mode),
        Some(_) => return Err(AgentError::UnknownMode),
    };
    if let Some(perm) = next_perm
        && idle_for_swap
        && perm != process_mode
    {
        return swap_idle_settings(shared, next_model, next_effort, perm).await;
    }
    let mut switched_model = false;
    if let Some(model) = &requested.model {
        let reply = shared
            .rpc()
            .ask(
                "session/set_config_option",
                json!({ "sessionId": session_id, "configId": "model", "value": model.as_str() }),
            )
            .await?;
        launch::update_options(shared, &reply);
        shared.state.lock().model.clone_from(model);
        switched_model = true;
    }
    if (requested.effort.is_some() || requested.model.is_some())
        && let Err(error) = apply_effort(shared, &session_id, next_effort, level, requested.effort.is_some()).await
    {
        rollback_config(shared, &session_id, switched_model.then_some(&current_model), None).await;
        return Err(error);
    }
    if let Some(perm) = next_perm {
        let changed = {
            let mut state = shared.state.lock();
            let changed = state.mode != perm;
            state.mode = perm;
            changed
        };
        if changed {
            shared.event(EventBody::AgentMode { mode: perm }, None).await;
        }
    }
    Ok(())
}

async fn swap_idle_settings(
    shared: &Arc<Shared>,
    model: &ModelName,
    effort: settings::Effort,
    perm: ApprovalMode,
) -> Result<(), AgentError> {
    let changed = {
        let mut state = shared.state.lock();
        state.model.clone_from(model);
        state.effort = effort;
        let changed = state.mode != perm;
        state.mode = perm;
        changed
    };
    ensure_mode_process(shared).await?;
    if changed {
        shared.event(EventBody::AgentMode { mode: perm }, None).await;
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
            .rpc()
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
        if let Ok(level) = effort_str(&effort) {
            let _ = apply_effort(shared, session_id, effort.clone(), level, false).await;
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
            .rpc()
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

async fn cancel(shared: &Arc<Shared>) -> Result<(), AgentError> {
    let (open, before) = between_endings(shared, |state| {
        let active_idle = state.idle_own_turn == super::wakeup::IdleOwnTurn::Active;
        if state.prompts.is_empty() && state.ending == Ending::Not && state.waiting.is_empty() && !active_idle {
            return Ok(None);
        }
        let has_prompts = !state.prompts.is_empty();
        state.cancelling = has_prompts || active_idle;
        Ok(Some((state.session_id.clone(), has_prompts, active_idle)))
    })
    .await?;
    let Some((session_id, has_prompts, active_idle)) = open else { return Ok(()) };
    // Waits on a count, and the turn may have ended already, which `wait_for` sees in the count before it waits.
    let mut done = shared.turn_done.subscribe();
    approvals::cancel_native_waiters(shared).await;
    if !has_prompts {
        if active_idle {
            if let Some(session_id) = session_id {
                let _ = shared.rpc().notify("session/cancel", json!({ "sessionId": session_id })).await;
            }
            advance_transcript_floor(shared).await;
            let mut state = shared.state.lock();
            state.idle_own_turn = super::wakeup::IdleOwnTurn::None;
            state.cancelling = false;
        }
        notifications::flush_streams(shared).await;
        turn::settle_turn(shared).await;
        turn::record_end(shared, TurnOutcome::Cancelled, None, None, None).await;
        return Ok(());
    }
    if let Some(session_id) = session_id
        && let Err(error) = shared.rpc().notify("session/cancel", json!({ "sessionId": session_id })).await
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

async fn advance_transcript_floor(shared: &Shared) {
    let path = shared.state.lock().transcript.as_ref().map(|cursor| cursor.path.clone());
    if let Some(path) = path
        && let Ok(meta) = tokio::fs::metadata(path).await
        && let Some(cursor) = shared.state.lock().transcript.as_mut()
    {
        cursor.wakeup_floor = cursor.wakeup_floor.max(meta.len());
    }
}
