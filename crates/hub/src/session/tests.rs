use protocol::event::{AgentRunState, AgentTask, ApprovalOutcome, ExitReason, TaskKind, TurnOutcome};

use super::ingest::{heard_before, says_something_new};
use super::status::{Open, after_background, next_status, turn_edge, works_unattended};
use super::*;
use crate::store::TurnEdge;

const NOTHING: Open = Open { approvals: 0, tasks: 0, agent: true };
const ONE_APPROVAL: Open = Open { approvals: 1, tasks: 0, agent: true };
const ONE_TASK: Open = Open { approvals: 0, tasks: 1, agent: true };

fn turn_ended() -> EventBody {
    EventBody::TurnEnded {
        outcome: TurnOutcome::Success,
        result: None,
        errors: vec![],
        duration_ms: None,
        cost_usd: None,
        num_turns: None,
        started_at_ms: None,
        models: Vec::new(),
        tokens: None,
        only_compacted: false,
    }
}

fn state(state: AgentRunState) -> EventBody {
    EventBody::AgentState { state }
}

fn exited() -> EventBody {
    EventBody::AgentExited { reason: ExitReason::Unexpected, code: None, signal: None, stderr_tail: vec![] }
}

fn tasks(count: usize) -> EventBody {
    let task =
        |i: usize| AgentTask { id: format!("t{i}").into(), description: None, kind: TaskKind::Command, call: None };
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
    EventBody::AgentOther { kind: "system/task_notification".into(), parent_tool_use_id: None }
}

/// The status after each event in turn, with what's open after each.
fn after(from: SessionStatus, events: &[(EventBody, Open)]) -> SessionStatus {
    events.iter().fold(from, |current, (body, open)| next_status(current, body, *open, false))
}

fn resolved() -> EventBody {
    EventBody::ApprovalResolved {
        approval_id: "a".into(),
        outcome: ApprovalOutcome::Allowed,
        answers_by_text: None,
        answers_by_position: None,
    }
}

use SessionStatus::*;

#[test]
fn a_prompt_starts_running_from_any_state() {
    let prompt = EventBody::UserPrompt { text: "x".into(), client_msg_id: None, attachments: Vec::new() };
    for from in [Idle, Running, WaitingApproval, Interrupted, Background] {
        assert_eq!(next_status(from, &prompt, NOTHING, false), Running);
    }
}

/// It starts a turn of its own, so the hub refuses anything else that needs the agent to be done until it ends.
#[test]
fn asking_to_compact_sets_running_as_a_prompt_does() {
    for from in [Idle, Interrupted, Background] {
        assert_eq!(next_status(from, &EventBody::CompactionAsked { keeping: None }, NOTHING, false), Running);
    }
}

#[test]
fn the_agents_own_state_decides_idle() {
    assert_eq!(next_status(Running, &state(AgentRunState::Idle), NOTHING, false), Idle);
    assert_eq!(next_status(Running, &state(AgentRunState::RequiresAction), NOTHING, false), WaitingApproval);
    assert_eq!(next_status(Idle, &state(AgentRunState::Running), NOTHING, false), Running);
}

#[test]
fn turn_end_still_goes_idle_if_the_agent_sends_no_state_events() {
    assert_eq!(next_status(Running, &turn_ended(), NOTHING, false), Idle);
}

#[test]
fn waiting_only_ends_when_the_last_approval_is_answered() {
    assert_eq!(next_status(WaitingApproval, &resolved(), ONE_APPROVAL, false), WaitingApproval);
    assert_eq!(next_status(WaitingApproval, &resolved(), NOTHING, false), Running);
    // A running state event doesn't hide an approval that's still open.
    assert_eq!(next_status(WaitingApproval, &state(AgentRunState::Running), ONE_APPROVAL, false), WaitingApproval);
}

#[test]
fn losing_the_agent_mid_turn_is_an_interruption_but_losing_it_when_idle_is_nothing() {
    assert_eq!(next_status(Running, &exited(), NOTHING, false), Interrupted);
    assert_eq!(next_status(WaitingApproval, &exited(), NOTHING, false), Interrupted);
    assert_eq!(next_status(Idle, &exited(), NOTHING, false), Idle);
    assert_eq!(next_status(Interrupted, &exited(), NOTHING, false), Interrupted);
}

#[test]
fn a_turn_that_leaves_work_in_the_background_keeps_the_agent_at_work() {
    assert_eq!(next_status(Running, &turn_ended(), ONE_TASK, false), Background);
    assert_eq!(next_status(Running, &state(AgentRunState::Idle), ONE_TASK, false), Background);
    assert_eq!(next_status(Background, &state(AgentRunState::Idle), ONE_TASK, false), Background);
    // It's still the end of a turn, so what the agent said is its reply.
    assert_eq!(turn_edge(Running, Background), TurnEdge::Ends);
    assert_eq!(turn_edge(Background, Idle), TurnEdge::Neither);
}

/// It picks up again by itself once that work is done, so it's still at work until it does, as the real CLI showed
/// for a command that finished or failed, a monitor whose command ended, and a subagent that reported.
#[test]
fn background_work_finishing_between_turns_leaves_the_agent_at_work_until_it_picks_up_again() {
    assert_eq!(next_status(Background, &tasks(0), NOTHING, false), Background);
    assert!(after_background(Background, Background, 1, 0));
    assert_eq!(next_status(Background, &tasks(2), Open { tasks: 2, ..NOTHING }, false), Background);
    assert!(!after_background(Background, Background, 3, 2), "some of it is still going");
    // Listed after the turn ended, rather than before.
    assert_eq!(next_status(Idle, &tasks(1), ONE_TASK, false), Background);
    // A list said again, with nothing in it, isn't the work going.
    assert_eq!(next_status(Idle, &tasks(0), NOTHING, false), Idle);
    assert!(!after_background(Background, Background, 0, 0));
}

/// As when the hub takes a wait off the list for an agent that's gone, which leaves nothing to pick up again.
#[test]
fn the_last_of_the_work_going_with_no_agent_running_leaves_the_chat_idle() {
    let no_agent = Open { agent: false, ..NOTHING };
    assert_eq!(next_status(Background, &tasks(0), no_agent, false), Idle);
    assert!(!after_background(Background, Idle, 1, 0));
    assert_eq!(next_status(Background, &tasks(1), Open { tasks: 1, ..no_agent }, false), Background);
}

/// The real CLI's lines once a command it left running finishes between turns, each with what's left running after
/// it: what became of the task, the list without it, then the agent saying it's running, and the turn's `init`.
#[test]
fn the_agent_is_at_work_throughout_the_real_clis_picking_up_again() {
    let lines =
        [(notified(), ONE_TASK), (tasks(0), NOTHING), (state(AgentRunState::Running), NOTHING), (init(), NOTHING)];
    let mut status = Background;
    for (body, open) in &lines {
        status = next_status(status, body, *open, false);
        assert!(status.is_active(), "{status:?} after {body:?}");
    }
    assert_eq!(status, Running);
}

/// A monitor that times out says it was stopped, and the agent picks up again after it as after any other.
#[test]
fn a_monitor_timing_out_between_turns_leaves_the_agent_at_work_until_it_picks_up_again() {
    let timed_out = [(notified(), ONE_TASK), (stopped(), NOTHING), (state(AgentRunState::Running), NOTHING)];
    assert_eq!(next_status(Background, &stopped(), NOTHING, false), Background);
    assert_eq!(after(Background, &timed_out), Running);
}

/// It hasn't, after a while, so the hub says it's idle, which a state event from the hub does as one from the agent
/// would.
#[test]
fn the_agent_taken_to_be_idle_once_it_hasnt_picked_up_again_is_idle() {
    assert_eq!(next_status(Background, &state(AgentRunState::Idle), NOTHING, false), Idle);
    assert_eq!(turn_edge(Background, Idle), TurnEdge::Neither);
}

#[test]
fn background_work_changing_mid_turn_leaves_the_turn_alone() {
    assert_eq!(next_status(Running, &tasks(1), ONE_TASK, false), Running);
    assert_eq!(next_status(Running, &tasks(0), NOTHING, false), Running);
    assert_eq!(next_status(WaitingApproval, &tasks(1), Open { tasks: 1, ..ONE_APPROVAL }, false), WaitingApproval);
    assert_eq!(next_status(Interrupted, &tasks(1), ONE_TASK, false), Interrupted);
}

#[test]
fn a_prompt_or_the_agent_picking_up_again_starts_a_turn_over_background_work() {
    let prompt = EventBody::UserPrompt { text: "x".into(), client_msg_id: None, attachments: Vec::new() };
    assert_eq!(next_status(Background, &prompt, ONE_TASK, false), Running);
    assert_eq!(next_status(Background, &state(AgentRunState::Running), ONE_TASK, false), Running);
}

#[test]
fn losing_the_agent_with_work_in_the_background_is_an_interruption() {
    assert_eq!(next_status(Background, &exited(), NOTHING, false), Interrupted);
    // On its way out, it stops that work first, and that isn't the work finishing.
    assert_eq!(next_status(Background, &stopped(), NOTHING, false), Background);
}

#[test]
fn a_turn_the_agent_starts_by_itself_once_its_work_finishes_is_running() {
    // Its last task finishing, then the turn it starts to take that in, with no prompt and no state event.
    let finished = [(notified(), ONE_TASK), (tasks(0), NOTHING), (init(), NOTHING)];
    assert_eq!(after(Background, &finished), Running);
    // The list can come once the turn has started, too.
    assert_eq!(after(Background, &[(init(), ONE_TASK), (tasks(0), NOTHING)]), Running);
    // A turn starting doesn't hide an approval that's still open.
    assert_eq!(next_status(WaitingApproval, &init(), ONE_APPROVAL, false), WaitingApproval);
}

#[test]
fn taking_a_prompt_after_the_turn_ended_is_running_again() {
    // A prompt sent while the agent writes its last reply becomes a turn of its own, with no state event to say so.
    let taken = EventBody::PromptTaken { prompt: 7 };

    assert_eq!(next_status(Idle, &taken, NOTHING, false), Running);
    assert_eq!(next_status(Background, &taken, ONE_TASK, false), Running);
    assert_eq!(next_status(WaitingApproval, &taken, ONE_APPROVAL, false), WaitingApproval);
    assert_eq!(next_status(Interrupted, &taken, NOTHING, false), Interrupted);
}

#[test]
fn an_idle_from_before_a_prompt_the_agent_has_yet_to_take_changes_nothing() {
    assert_eq!(next_status(Running, &state(AgentRunState::Idle), NOTHING, true), Running);
    assert_eq!(next_status(Running, &state(AgentRunState::Idle), NOTHING, false), Idle);
}

#[test]
fn a_turn_starts_when_the_agent_starts_working_and_ends_when_it_comes_to_rest() {
    use TurnEdge::{Ends, Neither, Starts};

    for from in [Idle, Interrupted, Unknown] {
        assert_eq!(turn_edge(from, Running), Starts, "from {from:?}");
    }
    assert_eq!(turn_edge(Background, Running), Starts, "picking up again after work in the background is a new turn");
    let taken = EventBody::PromptTaken { prompt: 3 };
    assert_eq!(
        turn_edge(Idle, next_status(Idle, &taken, NOTHING, false)),
        Starts,
        "a prompt taken after the turn it came in has ended starts its own"
    );
    assert_eq!(turn_edge(Idle, WaitingApproval), Starts);
    assert_eq!(turn_edge(Running, Idle), Ends);
    assert_eq!(turn_edge(WaitingApproval, Idle), Ends);
    assert_eq!(turn_edge(Running, WaitingApproval), Neither, "asking is part of the turn");
    assert_eq!(turn_edge(WaitingApproval, Running), Neither);
    assert_eq!(turn_edge(Running, Interrupted), Neither, "a turn the agent was cut off from doesn't end");
}

fn context(used_tokens: u64) -> ContextUsage {
    ContextUsage { used_tokens, max_tokens: 200_000, parts: Vec::new() }
}

#[test]
fn only_the_run_going_says_how_full_the_context_is_and_only_a_change_counts() {
    let (going, replaced) = (RunId::new(2), RunId::new(1));

    assert!(says_something_new(Some(going), None, going, &context(1)), "the first report");
    assert!(says_something_new(Some(going), Some(&context(1)), going, &context(2)));
    assert!(
        !says_something_new(Some(going), Some(&context(1)), going, &context(1)),
        "the feed isn't told what it knows"
    );
    assert!(!says_something_new(Some(going), Some(&context(1)), replaced, &context(2)), "a replaced run's");
    assert!(!says_something_new(None, Some(&context(1)), going, &context(2)), "a run that has exited");
}

#[test]
fn hearing_of_the_accounts_limits_changes_no_status() {
    let heard = EventBody::AgentLimits { limits: Vec::new() };

    for status in [Idle, Running, WaitingApproval, Background, Interrupted] {
        assert_eq!(next_status(status, &heard, NOTHING, false), status);
    }
}

#[test]
fn only_what_goes_on_without_a_person_is_working_unattended() {
    for status in [Running, Background] {
        assert!(works_unattended(status, 0), "{status:?}");
    }
    for status in [Idle, Interrupted, Unknown, WaitingApproval] {
        assert!(!works_unattended(status, 0), "{status:?}");
    }
    assert!(works_unattended(WaitingApproval, 1), "what it left running before it asked goes on");
}

/// As a worker sends a hub that connects to it what it sent the last hub, which may have recorded some of it.
#[test]
fn what_a_worker_sends_again_is_taken_in_once() {
    let (first, next) = (RunId::new(3), RunId::new(4));
    let mut recorded = Some((first, 5));
    let heard: Vec<_> = [4, 5, 6, 7].into_iter().map(|seq| heard_before(&mut recorded, first, seq)).collect();
    assert_eq!(heard, [true, true, false, false], "only what came after the last recorded");
    assert_eq!(recorded, Some((first, 7)));
    assert!(heard_before(&mut recorded, first, 7), "and each only once");
    assert!(!heard_before(&mut recorded, next, 2), "another run's is never the same, whatever its number");
    assert_eq!(recorded, Some((next, 2)));
    let mut never = None;
    assert!(!heard_before(&mut never, first, 1), "nothing is before the first");
}
