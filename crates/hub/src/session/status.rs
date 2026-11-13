//! What an event does to a session's status, and to what its agent has left open.

use std::collections::HashMap;

use protocol::event::{AgentRunState, AgentTask, EventBody};
use protocol::model::{ApprovalId, RunId, SessionStatus};

use crate::store::TurnEdge;

impl super::LiveSession {
    /// Whether the agent is doing something that goes on without a person.
    pub(crate) fn works_unattended(&self) -> bool {
        let progress = &self.state.lock().progress;
        works_unattended(progress.status, progress.background_tasks.len())
    }

    /// As this hub stops, for a session with a wait open: has its agent's exit leave it as the next hub's `recover`
    /// would, if its only work is the wait, still at work in the background, with the wait listed and its queue not
    /// held. Its agent goes with this hub's workers, and the next hub starts it again once the wait ends.
    pub(crate) async fn outlive_this_hub(&self) {
        // After what's being recorded, which the store has before this state does, so a turn that has just ended
        // counts as ended.
        let _in_order = self.recording.lock().await;
        self.state.lock().progress.outlive_this_hub(true);
    }

    /// Whether the hub counts this session's agent as running, so a hub asked to stop only if nothing would notice
    /// carries on for it, as [`SessionStatus::counts_as_running`] says.
    pub(crate) fn counts_as_running(&self) -> bool {
        let progress = &self.state.lock().progress;
        progress.status.counts_as_running(&progress.background_tasks)
    }
}

/// What the agent has left open once an event is applied.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Open {
    pub(crate) approvals: usize,
    /// Work it's running in the background.
    pub(crate) tasks: usize,
    /// Whether it's running, so it can pick up again by itself once the last of that work has gone.
    pub(crate) agent: bool,
}

/// The status after an event. A prompt sent mid-turn folds into that turn and produces no result of
/// its own, so `AgentState` is what says when the agent is really idle. `TurnEnded` also goes to
/// idle, in case a future CLI stops sending it, and any state event after that corrects it. Work left
/// running in the background keeps the agent at work after its turn ends, since it picks up again by
/// itself when that finishes, and the last of it going between turns leaves a running agent at work until it does,
/// as [`after_background`] says. `AgentInit` starts every turn, and is the only sign of one the agent starts by
/// itself, which comes with no prompt and no state event. A prompt sent while the agent writes its last reply
/// isn't folded in: the turn ends, and the agent takes the prompt as a turn of its own without saying it's running
/// again, so taking a prompt says it. An `idle` that's `stale` comes from before a prompt the agent has yet to take,
/// which it will, so it changes nothing.
pub(crate) fn next_status(current: SessionStatus, body: &EventBody, open: Open, stale: bool) -> SessionStatus {
    let between = if open.tasks > 0 { SessionStatus::Background } else { SessionStatus::Idle };
    match body {
        EventBody::AgentState { state: AgentRunState::Idle } if stale => current,
        // Either starts a turn, whose end says the agent is done.
        EventBody::UserPrompt { .. } | EventBody::CompactionAsked { .. } => SessionStatus::Running,
        EventBody::PromptTaken { .. } if current.between_turns() => SessionStatus::Running,
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
        EventBody::AgentState { state: AgentRunState::Idle } | EventBody::TurnEnded { .. } => between,
        // Between turns. During one, its end decides. With no agent running, as when the hub takes a wait off the list
        // for one that's gone, nothing will pick up.
        EventBody::AgentTasks { .. } if current.between_turns() => {
            if open.tasks > 0 || !open.agent {
                between
            } else {
                current
            }
        }
        EventBody::ApprovalResolved { .. } if current == SessionStatus::WaitingApproval && open.approvals == 0 => {
            SessionStatus::Running
        }
        EventBody::AgentExited { .. } if current.is_active() => SessionStatus::Interrupted,
        EventBody::AgentState { state: AgentRunState::Unknown }
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::AgentStarted { .. }
        | EventBody::AgentTasks { .. }
        | EventBody::AgentLimits { .. }
        | EventBody::Assistant { .. }
        | EventBody::AgentUser { .. }
        | EventBody::ApprovalResolved { .. }
        | EventBody::AgentExited { .. }
        | EventBody::Notice { .. }
        | EventBody::RequestFailed { .. }
        | EventBody::AgentMode { .. }
        | EventBody::SubagentStarted { .. }
        | EventBody::SubagentContext { .. }
        | EventBody::SubagentEnded { .. }
        | EventBody::AgentOther { .. }
        | EventBody::AgentCompacting { .. }
        | EventBody::AgentCompacted { .. }
        | EventBody::CacheExpired { .. }
        | EventBody::Unknown => current,
    }
}

/// Whether an event that took the session's status `from` one `to` another, and the work in the background from `had`
/// tasks to `has`, left it waiting for what the agent does once the last of that work has gone between turns. However
/// that work ended, the CLI says next either that it's running again, in a turn of its own that takes in how the work
/// ended, or that it has exited, as it does once it's been asked to stop and has stopped its work too. Even a monitor
/// that times out says it was stopped, and the agent picks up again after it. It starts that turn within a moment, so
/// the agent is still at work in between, and nothing takes the moment for its being done.
pub(crate) fn after_background(from: SessionStatus, to: SessionStatus, had: usize, has: usize) -> bool {
    from == SessionStatus::Background && to == from && had > 0 && has == 0
}

/// Whether an agent in this status, with this much running in the background, is doing something that goes on without
/// a person, so the machine it's on has to stay awake for it. Waiting on an answer or an approval isn't, since it could
/// wait all night, but what the agent left running before it asked still is. So it isn't whether the agent finishes by
/// itself, which one waiting on someone never does, whatever it left running.
pub(crate) fn works_unattended(status: SessionStatus, background_tasks: usize) -> bool {
    match status {
        SessionStatus::Running | SessionStatus::Background => true,
        SessionStatus::WaitingApproval => background_tasks > 0,
        SessionStatus::Idle | SessionStatus::Interrupted | SessionStatus::Unknown => false,
    }
}

/// What going from `current` to `next` does to the session's turns. A turn starts when the agent starts working from
/// anything else, including picking up again by itself after work in the background. It ends when the agent is
/// between turns, idle or with that work still going, which makes what it said its newest reply. A turn the agent was
/// cut off from doesn't end, since the session shows it as interrupted instead.
pub(crate) fn turn_edge(current: SessionStatus, next: SessionStatus) -> TurnEdge {
    match (current.in_turn(), next.in_turn()) {
        (false, true) => TurnEdge::Starts,
        (true, false) if next.between_turns() => TurnEdge::Ends,
        _ => TurnEdge::Neither,
    }
}

/// Open approvals, by the run that asked. `None` means an earlier hub's, which can't be answered.
pub(crate) type Approvals = HashMap<ApprovalId, Option<RunId>>;

/// What the agent has left open once this event, from `run`, is applied to what was open before it: the approvals it's
/// waiting on, and what it's running in the background. That goes with the agent that started it, which stops it when
/// it's stopped.
pub(crate) fn open_after(
    approvals: &Approvals,
    tasks: &[AgentTask],
    run: Option<RunId>,
    body: &EventBody,
) -> (Approvals, Vec<AgentTask>) {
    let (mut approvals, mut tasks) = (approvals.clone(), tasks.to_vec());
    match body {
        EventBody::ApprovalRequested { approval_id, .. } => {
            approvals.insert(approval_id.clone(), run);
        }
        EventBody::ApprovalResolved { approval_id, .. } => {
            approvals.remove(approval_id);
        }
        EventBody::AgentTasks { tasks: listed, .. } => tasks.clone_from(listed),
        EventBody::AgentStarted { .. } | EventBody::AgentExited { .. } => tasks.clear(),
        EventBody::UserPrompt { .. }
        | EventBody::CompactionAsked { .. }
        | EventBody::PromptTaken { .. }
        | EventBody::PromptDropped { .. }
        | EventBody::ToolCallDropped { .. }
        | EventBody::AgentInit { .. }
        | EventBody::AgentState { .. }
        | EventBody::AgentLimits { .. }
        | EventBody::Assistant { .. }
        | EventBody::AgentUser { .. }
        | EventBody::TurnEnded { .. }
        | EventBody::Notice { .. }
        | EventBody::RequestFailed { .. }
        | EventBody::AgentMode { .. }
        | EventBody::SubagentStarted { .. }
        | EventBody::SubagentContext { .. }
        | EventBody::SubagentEnded { .. }
        | EventBody::AgentOther { .. }
        | EventBody::AgentCompacting { .. }
        | EventBody::AgentCompacted { .. }
        | EventBody::CacheExpired { .. }
        | EventBody::Unknown => {}
    }
    (approvals, tasks)
}
