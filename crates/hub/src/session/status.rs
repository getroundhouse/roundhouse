//! What an event does to a session's status.

use std::collections::HashMap;

use protocol::event::{AgentRunState, AgentTask, EventBody};
use protocol::model::{ApprovalId, RunId, SessionStatus};

use crate::store::TurnEdge;

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
/// itself, which comes with no prompt and no state event. A prompt sent while the agent writes its last reply
/// isn't folded in: the turn ends, and the agent takes the prompt as a turn of its own without saying it's running
/// again, so taking a prompt says it. An `idle` that's `stale` comes from before a prompt the agent has yet to take,
/// which it will, so it changes nothing.
pub(crate) fn next_status(current: SessionStatus, body: &EventBody, open: Open, stale: bool) -> SessionStatus {
    let at_rest = if open.tasks > 0 { SessionStatus::Background } else { SessionStatus::Idle };
    match body {
        EventBody::AgentState { state: AgentRunState::Idle } if stale => current,
        EventBody::UserPrompt { .. } => SessionStatus::Running,
        EventBody::PromptTaken { .. } if matches!(current, SessionStatus::Idle | SessionStatus::Background) => {
            SessionStatus::Running
        }
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
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::AgentLimits { .. }
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

/// What going from `current` to `next` does to the session's turns. A turn starts when the agent starts working from
/// anything else, including picking up again by itself after work in the background. It ends when the agent comes to
/// rest, idle or with that work still going, which makes what it said its newest reply. A turn the agent was cut off
/// from doesn't end, since the session shows it as interrupted instead.
pub(crate) fn turn_edge(current: SessionStatus, next: SessionStatus) -> TurnEdge {
    match (current.in_turn(), next.in_turn()) {
        (false, true) => TurnEdge::Starts,
        (true, false) if matches!(next, SessionStatus::Idle | SessionStatus::Background) => TurnEdge::Ends,
        _ => TurnEdge::Neither,
    }
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
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::AgentLimits { .. }
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
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentLimits { .. }
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
