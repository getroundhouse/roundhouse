//! The line a change of approval mode leaves in the log.

use protocol::capability::approvals::ApprovalMode;

/// What a change to `mode` says. Every agent has full access unless it's planning, so going to full access is leaving
/// plan mode, unless the agent was in a mode of its own, such as one a chat chose before then. `before` is the mode it
/// changed from, when the log says.
pub(super) fn mode_change(before: Option<ApprovalMode>, mode: ApprovalMode) -> &'static str {
    let of_its_own = before.is_some_and(|before| !before.plan && !before.is_full_access());
    if mode.plan {
        "Plan mode on"
    } else if mode.is_full_access() && of_its_own {
        "Full access on"
    } else if mode.is_full_access() {
        "Plan mode off"
    } else {
        "Approval mode changed"
    }
}

#[cfg(test)]
mod tests {
    use protocol::capability::approvals::ApprovalMode;
    use protocol::fixtures::{full_access, plan_mode};

    use super::mode_change;

    #[test]
    fn full_access_is_plan_mode_off_unless_the_agent_was_in_a_mode_of_its_own() {
        let (plan, full, unknown) = (plan_mode(), full_access(), ApprovalMode::UNKNOWN);

        assert_eq!(mode_change(None, plan), "Plan mode on");
        assert_eq!(mode_change(Some(plan), full), "Plan mode off");
        assert_eq!(mode_change(None, full), "Plan mode off", "what it changed from isn't loaded");
        assert_eq!(mode_change(Some(unknown), full), "Full access on");
        assert_eq!(mode_change(Some(full), unknown), "Approval mode changed");
    }
}
