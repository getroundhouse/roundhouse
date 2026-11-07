//! What an event does to a session's status.

use std::collections::HashMap;

use protocol::event::{AgentRunState, EventBody};
use protocol::model::SessionStatus;

/// The status after an event. A prompt sent mid-turn folds into that turn and produces no result of
/// its own, so `AgentState` is what says when the agent is really idle. `TurnEnded` also goes to
/// idle, in case a future CLI stops sending it, and any state event after that corrects it.
pub(crate) fn next_status(current: SessionStatus, body: &EventBody, approvals_left: usize) -> SessionStatus {
    match body {
        EventBody::UserPrompt { .. } => SessionStatus::Running,
        EventBody::AgentState { state: AgentRunState::Running } => {
            if approvals_left > 0 {
                SessionStatus::WaitingApproval
            } else {
                SessionStatus::Running
            }
        }
        EventBody::AgentState { state: AgentRunState::RequiresAction } | EventBody::ApprovalRequested { .. } => {
            SessionStatus::WaitingApproval
        }
        EventBody::AgentState { state: AgentRunState::Idle } | EventBody::TurnEnded { .. } => SessionStatus::Idle,
        EventBody::ApprovalResolved { .. } if current == SessionStatus::WaitingApproval && approvals_left == 0 => {
            SessionStatus::Running
        }
        EventBody::AgentExited { .. } if matches!(current, SessionStatus::Running | SessionStatus::WaitingApproval) => {
            SessionStatus::Interrupted
        }
        EventBody::AgentState { state: AgentRunState::Unknown }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::Assistant { .. }
        | EventBody::AgentUser { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::AgentExited { .. }
        | EventBody::Notice { .. }
        | EventBody::AgentMode { .. }
        | EventBody::AgentOther { .. }
        | EventBody::Unknown => current,
    }
}

/// How many approvals stay open once this event is applied.
pub(crate) fn approvals_after(pending: &HashMap<String, u64>, body: &EventBody) -> usize {
    match body {
        EventBody::ApprovalRequested { approval_id, .. } if !pending.contains_key(approval_id) => pending.len() + 1,
        EventBody::ApprovalResolved { approval_id, .. } if pending.contains_key(approval_id) => pending.len() - 1,
        EventBody::UserPrompt { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::Assistant { .. }
        | EventBody::AgentUser { .. }
        | EventBody::TurnEnded { .. }
        | EventBody::ApprovalRequested { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::AgentExited { .. }
        | EventBody::Notice { .. }
        | EventBody::AgentMode { .. }
        | EventBody::AgentOther { .. }
        | EventBody::Unknown => pending.len(),
    }
}
