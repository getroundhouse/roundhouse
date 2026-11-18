//! The approval modes the CLI reports, in the captured output.

use protocol::capability::approvals::ApprovalMode;
use protocol::event::EventBody;
use protocol::fixtures::plan_mode;

use super::events;

#[test]
fn every_mode_change_is_reported() {
    let modes: Vec<ApprovalMode> = events()
        .into_iter()
        .filter_map(|e| match e {
            EventBody::AgentMode { mode } => Some(mode),
            _ => None,
        })
        .collect();
    let expected = [plan_mode(), ApprovalMode::UNKNOWN];
    assert_eq!(modes, expected, "chosen, then left by the CLI itself for the mode before when the plan was approved");
}

/// A launch that's to plan relies on it, to tell whether the CLI started a turn before it went into plan mode.
#[test]
fn a_turn_after_a_switch_into_plan_mode_starts_in_it() {
    let after_the_switch = events()
        .into_iter()
        .skip_while(|e| !matches!(e, EventBody::AgentMode { mode } if mode.plan))
        .find_map(|e| match e {
            EventBody::AgentInit { mode, .. } => Some(mode),
            _ => None,
        });
    assert_eq!(after_the_switch, Some(Some(plan_mode())));
}
