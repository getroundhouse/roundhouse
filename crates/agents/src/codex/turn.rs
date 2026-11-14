use std::time::Duration;

use protocol::model::PermissionMode;
use serde_json::{Value, json};

use super::{running::Shared, state::TurnId};
use crate::AgentError;

pub(super) async fn send(shared: &Shared, input: Vec<Value>) -> Result<(), AgentError> {
    let (thread, active) = {
        let state = shared.state.lock();
        (state.thread.clone().ok_or(AgentError::Exited)?, state.turn.clone())
    };
    if let Some(active) = active {
        let mut done = shared.turn_done.subscribe();
        match shared.rpc.ask("turn/steer", json!({"threadId":thread,"expectedTurnId":active,"input":input})).await {
            Ok(_) => return Ok(()),
            // Native intake and completion can cross on the wire. This refusal proves the input was not accepted;
            // finish that boundary before starting its next turn once, without retaining a second prompt queue.
            Err(AgentError::Refused(message)) if message == "no active turn" => {
                tokio::time::timeout(Duration::from_secs(30), done.wait_for(|id| id.as_ref() == Some(&active)))
                    .await
                    .map_err(|_| AgentError::NoAnswer)?
                    .map_err(|_| AgentError::Exited)?;
            }
            Err(error) => return Err(error),
        }
    }
    start(shared, input).await
}

async fn start(shared: &Shared, input: Vec<Value>) -> Result<(), AgentError> {
    let (thread, settings) = {
        let mut state = shared.state.lock();
        if !state.waiting.is_empty() {
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
    if shared.turn_done.borrow().as_ref() != Some(&id) {
        state.turn = Some(id);
    }
    Ok(())
}
