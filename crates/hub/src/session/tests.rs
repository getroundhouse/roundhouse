use protocol::event::{AgentRunState, AgentTask, ApprovalOutcome, ExitReason, TurnOutcome};

use super::status::{Open, ends_turn, next_status};
use super::*;

const NOTHING: Open = Open { approvals: 0, tasks: 0 };
const ONE_APPROVAL: Open = Open { approvals: 1, tasks: 0 };
const ONE_TASK: Open = Open { approvals: 0, tasks: 1 };

fn turn_ended() -> EventBody {
    EventBody::TurnEnded {
        outcome: TurnOutcome::Success,
        result: None,
        errors: vec![],
        duration_ms: None,
        cost_usd: None,
        num_turns: None,
    }
}

fn state(state: AgentRunState) -> EventBody {
    EventBody::AgentState { state }
}

fn exited() -> EventBody {
    EventBody::AgentExited { reason: ExitReason::Unexpected, code: None, signal: None, stderr_tail: vec![] }
}

fn tasks(count: usize) -> EventBody {
    let task = |i: usize| AgentTask { id: format!("t{i}").into(), description: None };
    EventBody::AgentTasks { tasks: (0..count).map(task).collect(), stopped: Vec::new() }
}

/// The last task gone, because it was stopped.
fn stopped() -> EventBody {
    EventBody::AgentTasks { tasks: Vec::new(), stopped: vec!["t0".into()] }
}

/// What starts every turn, including one the agent starts by itself.
fn init() -> EventBody {
    EventBody::AgentInit { agent_session_id: None, model: None, cwd: None, permission_mode: None, agent_version: None }
}

fn notified() -> EventBody {
    EventBody::AgentOther { kind: "system/task_notification".into() }
}

/// The status after each event in turn, with what's open after each.
fn after(from: SessionStatus, events: &[(EventBody, Open)]) -> SessionStatus {
    events.iter().fold(from, |current, (body, open)| next_status(current, body, *open))
}

fn resolved() -> EventBody {
    EventBody::ApprovalResolved { approval_id: "a".into(), outcome: ApprovalOutcome::Allowed }
}

use SessionStatus::*;

#[test]
fn a_prompt_starts_running_from_any_state() {
    let prompt = EventBody::UserPrompt { text: "x".into(), client_msg_id: None };
    for from in [Idle, Running, WaitingApproval, Interrupted, Background] {
        assert_eq!(next_status(from, &prompt, NOTHING), Running);
    }
}

#[test]
fn the_agents_own_state_decides_idle() {
    assert_eq!(next_status(Running, &state(AgentRunState::Idle), NOTHING), Idle);
    assert_eq!(next_status(Running, &state(AgentRunState::RequiresAction), NOTHING), WaitingApproval);
    assert_eq!(next_status(Idle, &state(AgentRunState::Running), NOTHING), Running);
}

#[test]
fn turn_end_still_goes_idle_if_the_agent_sends_no_state_events() {
    assert_eq!(next_status(Running, &turn_ended(), NOTHING), Idle);
}

#[test]
fn waiting_only_ends_when_the_last_approval_is_answered() {
    assert_eq!(next_status(WaitingApproval, &resolved(), ONE_APPROVAL), WaitingApproval);
    assert_eq!(next_status(WaitingApproval, &resolved(), NOTHING), Running);
    // A running state event doesn't hide an approval that's still open.
    assert_eq!(next_status(WaitingApproval, &state(AgentRunState::Running), ONE_APPROVAL), WaitingApproval);
}

#[test]
fn losing_the_agent_mid_turn_is_an_interruption_but_losing_it_when_idle_is_nothing() {
    assert_eq!(next_status(Running, &exited(), NOTHING), Interrupted);
    assert_eq!(next_status(WaitingApproval, &exited(), NOTHING), Interrupted);
    assert_eq!(next_status(Idle, &exited(), NOTHING), Idle);
    assert_eq!(next_status(Interrupted, &exited(), NOTHING), Interrupted);
}

#[test]
fn a_turn_that_leaves_work_in_the_background_keeps_the_agent_at_work() {
    assert_eq!(next_status(Running, &turn_ended(), ONE_TASK), Background);
    assert_eq!(next_status(Running, &state(AgentRunState::Idle), ONE_TASK), Background);
    assert_eq!(next_status(Background, &state(AgentRunState::Idle), ONE_TASK), Background);
    // It's still the end of a turn, so what the agent said is its reply.
    assert!(ends_turn(Running, Background));
    assert!(!ends_turn(Background, Idle));
}

#[test]
fn background_work_finishing_between_turns_leaves_the_agent_idle() {
    assert_eq!(next_status(Background, &tasks(0), NOTHING), Idle);
    assert_eq!(next_status(Background, &tasks(2), Open { approvals: 0, tasks: 2 }), Background);
    // Listed after the turn ended, rather than before.
    assert_eq!(next_status(Idle, &tasks(1), ONE_TASK), Background);
}

#[test]
fn background_work_changing_mid_turn_leaves_the_turn_alone() {
    assert_eq!(next_status(Running, &tasks(1), ONE_TASK), Running);
    assert_eq!(next_status(Running, &tasks(0), NOTHING), Running);
    assert_eq!(next_status(WaitingApproval, &tasks(1), Open { approvals: 1, tasks: 1 }), WaitingApproval);
    assert_eq!(next_status(Interrupted, &tasks(1), ONE_TASK), Interrupted);
}

#[test]
fn a_prompt_or_the_agent_picking_up_again_starts_a_turn_over_background_work() {
    let prompt = EventBody::UserPrompt { text: "x".into(), client_msg_id: None };
    assert_eq!(next_status(Background, &prompt, ONE_TASK), Running);
    assert_eq!(next_status(Background, &state(AgentRunState::Running), ONE_TASK), Running);
}

#[test]
fn losing_the_agent_with_work_in_the_background_is_an_interruption() {
    assert_eq!(next_status(Background, &exited(), NOTHING), Interrupted);
    // On its way out, it stops that work first, and that isn't the work finishing.
    assert_eq!(next_status(Background, &stopped(), NOTHING), Background);
}

#[test]
fn a_turn_the_agent_starts_by_itself_once_its_work_finishes_is_running() {
    // Its last task finishing, then the turn it starts to take that in, with no prompt and no state event.
    let finished = [(notified(), ONE_TASK), (tasks(0), NOTHING), (init(), NOTHING)];
    assert_eq!(after(Background, &finished), Running);
    // The list can come once the turn has started, too.
    assert_eq!(after(Background, &[(init(), ONE_TASK), (tasks(0), NOTHING)]), Running);
    // A turn starting doesn't hide an approval that's still open.
    assert_eq!(next_status(WaitingApproval, &init(), ONE_APPROVAL), WaitingApproval);
}
