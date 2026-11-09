//! What an event does to a session's status.

use std::collections::HashMap;

use protocol::event::{AgentRunState, AgentTask, EventBody};
use protocol::model::{ApprovalId, RunId, SessionStatus};

/// What the agent has left open once an event is applied.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Open {
    pub(crate) approvals: usize,
    /// Work it's running in the background.
    pub(crate) tasks: usize,
}

/// The status after an event. A prompt sent mid-turn folds into that turn and produces no result of
/// its own, so `AgentState` is what says when the agent is really idle. `TurnEnded` also goes to
/// idle, in case a future CLI stops sending it, and any state event after that corrects it. Work left
/// running in the background keeps the agent at work after its turn ends, since it picks up again by
/// itself when that finishes. `AgentInit` starts every turn, and is the only sign of one the agent starts by
/// itself, which comes with no prompt and no state event.
pub(crate) fn next_status(current: SessionStatus, body: &EventBody, open: Open) -> SessionStatus {
    let at_rest = if open.tasks > 0 { SessionStatus::Background } else { SessionStatus::Idle };
    match body {
        EventBody::UserPrompt { .. } => SessionStatus::Running,
        EventBody::AgentState { state: AgentRunState::Running } | EventBody::AgentInit { .. } => {
            if open.approvals > 0 {
                SessionStatus::WaitingApproval
            } else {
                SessionStatus::Running
            }
        }
        EventBody::AgentState { state: AgentRunState::RequiresAction } | EventBody::ApprovalRequested { .. } => {
            SessionStatus::WaitingApproval
        }
        EventBody::AgentState { state: AgentRunState::Idle } | EventBody::TurnEnded { .. } => at_rest,
        // Between turns. During one, its end decides. When the last of them was stopped rather than finishing, the
        // agent is being stopped too, and its exit says so.
        EventBody::AgentTasks { stopped, .. } if matches!(current, SessionStatus::Idle | SessionStatus::Background) => {
            if open.tasks == 0 && !stopped.is_empty() {
                current
            } else {
                at_rest
            }
        }
        EventBody::ApprovalResolved { .. } if current == SessionStatus::WaitingApproval && open.approvals == 0 => {
            SessionStatus::Running
        }
        EventBody::AgentExited { .. } if current.is_active() => SessionStatus::Interrupted,
        EventBody::AgentState { state: AgentRunState::Unknown }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentTasks { .. }
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

/// Whether going from `current` to `next` ends a turn, which makes what the agent said its newest reply. A turn the
/// agent was cut off from isn't one, since the session shows it as interrupted instead.
pub(crate) fn ends_turn(current: SessionStatus, next: SessionStatus) -> bool {
    matches!(next, SessionStatus::Idle | SessionStatus::Background)
        && matches!(current, SessionStatus::Running | SessionStatus::WaitingApproval)
}

/// What stays open once this event is applied, from what was open before it, and what's then running in the
/// background.
pub(crate) fn open_after(
    pending: &HashMap<ApprovalId, Option<RunId>>,
    tasks: &[AgentTask],
    body: &EventBody,
) -> (Open, Vec<AgentTask>) {
    let tasks = tasks_after(tasks, body);
    (Open { approvals: approvals_after(pending, body), tasks: tasks.len() }, tasks)
}

/// How many approvals stay open once this event is applied.
fn approvals_after(pending: &HashMap<ApprovalId, Option<RunId>>, body: &EventBody) -> usize {
    match body {
        EventBody::ApprovalRequested { approval_id, .. } if !pending.contains_key(approval_id) => pending.len() + 1,
        EventBody::ApprovalResolved { approval_id, .. } if pending.contains_key(approval_id) => pending.len() - 1,
        EventBody::UserPrompt { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
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

/// What's running in the background once this event is applied. It goes with the agent that started it, which
/// stops it when it's stopped.
fn tasks_after(tasks: &[AgentTask], body: &EventBody) -> Vec<AgentTask> {
    match body {
        EventBody::AgentTasks { tasks, .. } => tasks.clone(),
        EventBody::AgentStarted { .. } | EventBody::AgentExited { .. } => Vec::new(),
        EventBody::UserPrompt { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::Assistant { .. }
        | EventBody::AgentUser { .. }
        | EventBody::TurnEnded { .. }
        | EventBody::ApprovalRequested { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::Notice { .. }
        | EventBody::AgentMode { .. }
        | EventBody::AgentOther { .. }
        | EventBody::Unknown => tasks.to_vec(),
    }
}
