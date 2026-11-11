//! The line a change of approval mode leaves in the log.

use protocol::model::PermissionMode;

/// What a change to `mode` says. Every agent has full access unless it's planning, so going to full access is leaving
/// plan mode, unless the agent was in a mode of its own, such as one a chat chose before then. `before` is the mode it
/// changed from, when the log says.
pub(super) fn mode_change(before: Option<PermissionMode>, mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::Plan => "Plan mode on",
        PermissionMode::FullAccess if before == Some(PermissionMode::Unknown) => "Full access on",
        PermissionMode::FullAccess => "Plan mode off",
        PermissionMode::Unknown => "Approval mode changed",
    }
}

#[cfg(test)]
mod tests {
    use protocol::model::PermissionMode::{FullAccess, Plan, Unknown};

    use super::mode_change;

    #[test]
    fn full_access_is_plan_mode_off_unless_the_agent_was_in_a_mode_of_its_own() {
        assert_eq!(mode_change(None, Plan), "Plan mode on");
        assert_eq!(mode_change(Some(Plan), FullAccess), "Plan mode off");
        assert_eq!(mode_change(None, FullAccess), "Plan mode off", "what it changed from isn't loaded");
        assert_eq!(mode_change(Some(Unknown), FullAccess), "Full access on");
        assert_eq!(mode_change(Some(FullAccess), Unknown), "Approval mode changed");
    }
}
