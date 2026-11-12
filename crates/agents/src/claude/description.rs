//! What Claude is called and offers, for the hub to tell clients.

use protocol::model::{
    AgentDescription, AgentFeature, AgentKind, Effort, ModelChoice, ModelName, PermissionMode, SessionSettings,
};

/// Aliases, so these stay the newest of each without a release here. There's no `default`, since a chat always says
/// which model it runs.
const MODELS: [(ModelName, &str); 4] = [
    (ModelName::named("fable"), "Fable"),
    (ModelName::named("opus"), "Opus"),
    (ModelName::named("sonnet"), "Sonnet"),
    (ModelName::named("haiku"), "Haiku"),
];

pub fn description() -> AgentDescription {
    AgentDescription {
        kind: AgentKind::Claude,
        name: "Claude Code".to_string(),
        usage_heading: "Claude usage".to_string(),
        models: Vec::from(MODELS.map(|(model, label)| ModelChoice { model, label: label.to_string() })),
        // Every level, since the CLI names each, as `effort` says.
        efforts: Effort::ALL.to_vec(),
        modes: PermissionMode::CHOSEN.to_vec(),
        defaults: SessionSettings::default(),
        // It's asked with `/compact`, as `process` sends it.
        features: vec![AgentFeature::Compacts],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_chat_starts_with_is_among_what_its_offered() {
        let claude = description();

        assert!(claude.label(&claude.defaults.model).is_some());
        assert!(claude.efforts.contains(&claude.defaults.effort));
        assert!(claude.modes.contains(&claude.defaults.permission_mode));
    }

    #[test]
    fn every_level_offered_has_a_name_the_cli_takes() {
        for effort in description().efforts {
            assert!(crate::claude::effort::level(effort).is_some(), "{effort:?}");
        }
    }
}
