//! The approval modes the CLI reports, in the captured output.

use protocol::event::EventBody;
use protocol::model::PermissionMode;

use super::events;

#[test]
fn every_mode_change_is_reported() {
    let modes: Vec<PermissionMode> = events()
        .into_iter()
        .filter_map(|e| match e {
            EventBody::AgentMode { permission_mode } => Some(permission_mode),
            _ => None,
        })
        .collect();
    let expected = [PermissionMode::Plan, PermissionMode::Unknown];
    assert_eq!(modes, expected, "chosen, then left by the CLI itself for the mode before when the plan was approved");
}

/// A launch that's to plan relies on it, to tell whether the CLI started a turn before it went into plan mode.
#[test]
fn a_turn_after_a_switch_into_plan_mode_starts_in_it() {
    let after_the_switch = events()
        .into_iter()
        .skip_while(|e| !matches!(e, EventBody::AgentMode { permission_mode: PermissionMode::Plan }))
        .find_map(|e| match e {
            EventBody::AgentInit { permission_mode, .. } => Some(permission_mode),
            _ => None,
        });
    assert_eq!(after_the_switch, Some(Some(PermissionMode::Plan)));
}
