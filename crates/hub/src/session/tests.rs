use protocol::event::{AgentRunState, ApprovalOutcome, ExitReason, TurnOutcome};

use super::record::title_from_prompt;
use super::status::next_status;
use super::*;

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

fn resolved() -> EventBody {
    EventBody::ApprovalResolved { approval_id: "a".into(), outcome: ApprovalOutcome::Allowed }
}

use SessionStatus::*;

#[test]
fn a_title_is_the_first_line_cut_at_a_word() {
    assert_eq!(title_from_prompt("\n  Fix the login bug\nand then the rest").as_deref(), Some("Fix the login bug"));
    assert_eq!(title_from_prompt("   \n "), None);
    let long = "word ".repeat(30);
    let title = title_from_prompt(&long).unwrap();
    assert!(title.ends_with("word…"), "{title}");
    assert!(title.chars().count() <= 49, "{title}");
}

#[test]
fn a_prompt_starts_running_from_any_state() {
    let prompt = EventBody::UserPrompt { text: "x".into(), client_msg_id: None };
    for from in [Idle, Running, WaitingApproval, Interrupted] {
        assert_eq!(next_status(from, &prompt, 0), Running);
    }
}

#[test]
fn the_agents_own_state_decides_idle() {
    assert_eq!(next_status(Running, &state(AgentRunState::Idle), 0), Idle);
    assert_eq!(next_status(Running, &state(AgentRunState::RequiresAction), 0), WaitingApproval);
    assert_eq!(next_status(Idle, &state(AgentRunState::Running), 0), Running);
}

#[test]
fn turn_end_still_goes_idle_if_the_agent_sends_no_state_events() {
    assert_eq!(next_status(Running, &turn_ended(), 0), Idle);
}

#[test]
fn waiting_only_ends_when_the_last_approval_is_answered() {
    assert_eq!(next_status(WaitingApproval, &resolved(), 1), WaitingApproval);
    assert_eq!(next_status(WaitingApproval, &resolved(), 0), Running);
    // A running state event doesn't hide an approval that's still open.
    assert_eq!(next_status(WaitingApproval, &state(AgentRunState::Running), 1), WaitingApproval);
}

#[test]
fn losing_the_agent_mid_turn_is_an_interruption_but_losing_it_when_idle_is_nothing() {
    assert_eq!(next_status(Running, &exited(), 0), Interrupted);
    assert_eq!(next_status(WaitingApproval, &exited(), 0), Interrupted);
    assert_eq!(next_status(Idle, &exited(), 0), Idle);
    assert_eq!(next_status(Interrupted, &exited(), 0), Interrupted);
}
