use std::time::Duration;

use protocol::model::PermissionMode;
use serde_json::{Value, json};

use super::{running::Shared, state::TurnId};
use crate::AgentError;

/// How long a turn's end is waited for once Codex has taken an interrupt of it, or said it's over by refusing a steer or
/// an interrupt. Codex sends the end as the turn finishes, so it's already on the way, and this only bounds a Codex that
/// broke that. It's as long as any answer from Codex is waited for, since a busy machine slows both alike.
pub(super) const END_WAIT: Duration = super::rpc::REPLY_WAIT;

pub(super) async fn send(shared: &Shared, input: Vec<Value>) -> Result<(), AgentError> {
    let (thread, active) = {
        let state = shared.state.lock();
        (state.thread.clone().ok_or(AgentError::Exited)?, state.turn.clone())
    };
    if let Some(active) = active {
        let mut done = shared.turn_done.subscribe();
        match shared.rpc.ask("turn/steer", json!({"threadId":thread,"expectedTurnId":active,"input":input})).await {
            Ok(_) => return Ok(()),
            // Taking a prompt and ending a turn can cross on the wire, which Codex refuses as having no turn to steer.
            // The refusal proves the prompt wasn't taken. Once the turn it was for has been heard ending, the prompt
            // starts the next one, once, without a queue of its own. Any other refusal is the prompt's, at once.
            Err(refusal) if super::rpc::no_active_turn(&refusal) => {
                match tokio::time::timeout(END_WAIT, done.wait_for(|id| id.as_ref() == Some(&active))).await {
                    Ok(Ok(_)) => {}
                    Ok(Err(_)) => return Err(AgentError::Exited),
                    Err(_) => return Err(refusal),
                }
            }
            Err(error) => return Err(error),
        }
    }
    start(shared, input).await
}

async fn start(shared: &Shared, input: Vec<Value>) -> Result<(), AgentError> {
    let (thread, settings) = {
        let mut state = shared.state.lock();
        if state.asking() {
            return Err(AgentError::Refused("Answer Codex's open request before sending another prompt".into()));
        }
        state.turn_mode = Some(state.settings.permission_mode);
        (state.thread.clone().ok_or(AgentError::Exited)?, state.settings.clone())
    };
    let plan = settings.permission_mode == PermissionMode::Plan;
    let response = shared.rpc.ask("turn/start", json!({
        "threadId":thread,"input":input,"model":settings.model.as_str(),"effort":settings.effort.as_str(),"summary":"auto",
        "approvalPolicy":if plan {"on-request"} else {"never"},
        "sandboxPolicy":if plan {json!({"type":"readOnly"})} else {json!({"type":"dangerFullAccess"})},
        "collaborationMode":{"mode":if plan {"plan"} else {"default"},
            "settings":{"model":settings.model.as_str(),"reasoning_effort":settings.effort.as_str(),
                "developer_instructions":if plan {Some("Present your finished plan inside <proposed_plan>...</proposed_plan>. Ask any blocking questions with request_user_input. Do not carry out the plan until it is approved.")} else {None::<&str>}}}
    })).await?;
    let id: TurnId = serde_json::from_value(response["turn"]["id"].clone())?;
    let mut state = shared.state.lock();
    if state.last_ended.as_ref() != Some(&id) {
        state.turn = Some(id);
    }
    Ok(())
}
