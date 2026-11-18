//! The names the command line gives approval modes.

use anyhow::bail;
use protocol::capability::approvals::ApprovalMode;
use protocol::model::AgentDescription;

/// A mode as the command line names it, by Claude's names for its two, whatever the agent: `plan`, in which it plans
/// first, and `bypassPermissions`, full access. Each names the mode of its kind among those an agent offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModeName {
    FullAccess,
    Plan,
}

impl ModeName {
    pub(crate) const ALL: [ModeName; 2] = [ModeName::FullAccess, ModeName::Plan];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ModeName::FullAccess => "bypassPermissions",
            ModeName::Plan => "plan",
        }
    }

    pub(crate) fn parse(name: &str) -> Option<ModeName> {
        ModeName::ALL.into_iter().find(|mode| mode.as_str() == name)
    }

    /// What names `mode`, if anything does.
    pub(crate) fn of(mode: ApprovalMode) -> Option<ModeName> {
        if !mode.is_known() {
            None
        } else if mode.plan {
            Some(ModeName::Plan)
        } else if mode.is_full_access() {
            Some(ModeName::FullAccess)
        } else {
            None
        }
    }

    /// The mode it names among those `agent` offers.
    pub(crate) fn among(self, agent: &AgentDescription) -> anyhow::Result<ApprovalMode> {
        match agent.modes().into_iter().find(|&mode| ModeName::of(mode) == Some(self)) {
            Some(mode) => Ok(mode),
            None => bail!("{} offers no mode called {}", agent.name, self.as_str()),
        }
    }
}

/// A mode nothing here names, such as one an agent went into by itself, or one from a newer hub, is `unknown`.
pub(crate) fn mode_name(mode: ApprovalMode) -> &'static str {
    ModeName::of(mode).map_or("unknown", ModeName::as_str)
}

#[cfg(test)]
mod tests {
    use protocol::capability::approvals::{Beyond, Modes, Unasked};
    use protocol::model::AgentKind;

    use super::*;

    fn offering(modes: Modes) -> AgentDescription {
        let mut claims = protocol::fixtures::claude_claims();
        claims.approvals.modes = Some(modes);
        AgentDescription {
            capabilities: claims,
            kind: AgentKind::Codex,
            name: "Codex".to_string(),
            ..protocol::fixtures::claude()
        }
    }

    #[test]
    fn a_mode_a_chat_cant_be_given_here_is_shown_as_unknown() {
        let asks = ApprovalMode { unasked: Unasked::Reads, ..protocol::fixtures::full_access() };

        let shown = [protocol::fixtures::plan_mode(), protocol::fixtures::full_access(), asks, ApprovalMode::UNKNOWN]
            .map(mode_name);

        assert_eq!(shown, ["plan", "bypassPermissions", "unknown", "unknown"]);
    }

    #[test]
    fn each_name_is_the_mode_of_its_kind_the_agent_offers() {
        let full = ApprovalMode { beyond: Beyond::Refused, ..protocol::fixtures::full_access() };
        let plan = ApprovalMode { beyond: Beyond::Refused, ..protocol::fixtures::plan_mode() };
        let agent = offering(Modes { default: full, others: vec![plan] });

        let named = ModeName::ALL.map(|name| name.among(&agent).unwrap());

        assert_eq!(named, [full, plan]);
    }

    #[test]
    fn a_name_an_agent_offers_no_mode_for_is_refused() {
        let agent = offering(Modes { default: protocol::fixtures::full_access(), others: Vec::new() });

        let error = ModeName::Plan.among(&agent).unwrap_err();

        assert_eq!(error.to_string(), "Codex offers no mode called plan");
    }
}
