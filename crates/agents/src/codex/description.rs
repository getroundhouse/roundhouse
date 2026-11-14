use protocol::model::{AgentDescription, AgentKind, Effort, ModelChoice, ModelName, PermissionMode, SessionSettings};

// A compiled snapshot of the native model/list catalog. Refresh it with the CLI observations in docs/codex-cli.md.
const MODELS: [(&str, &str, Effort, bool); 8] = [
    ("gpt-6.1-sol", "GPT-6.1 Sol", Effort::Low, true),
    ("gpt-6-astra", "GPT-6 Astra", Effort::Medium, true),
    ("gpt-6.1-terra", "GPT-6.1 Terra", Effort::Low, true),
    ("gpt-6-sol", "GPT-6 Sol", Effort::Medium, true),
    ("gpt-6-luna", "GPT-6 Luna", Effort::Medium, false),
    ("gpt-5.6-sol", "GPT-5.6 Sol", Effort::Low, true),
    ("gpt-5.6-terra", "GPT-5.6 Terra", Effort::Medium, true),
    ("gpt-5.6-luna", "GPT-5.6 Luna", Effort::Medium, false),
];

pub(super) fn models() -> protocol::capability::settings::Models {
    use protocol::capability::settings::{Input, ModelOffer, Models, Stage};
    Models {
        offered: MODELS
            .into_iter()
            .map(|(model, label, effort, ultra)| ModelOffer {
                model: ModelName::named(model),
                label: label.into(),
                description: None,
                model_provider: Some("OpenAI".into()),
                efforts: Effort::ALL
                    .into_iter()
                    .filter(|level| ultra || *level != Effort::Ultra)
                    .map(Into::into)
                    .collect(),
                default_effort: Some(effort.into()),
                speeds: Vec::new(),
                inputs: vec![Input::Images],
                context_tokens: None,
                price: None,
                stage: Stage::Current,
            })
            .collect(),
    }
}

pub fn description() -> AgentDescription {
    AgentDescription {
        kind: AgentKind::Codex,
        capabilities: Some(super::claims::claims()),
        name: "Codex".into(),
        usage_heading: "Codex usage".into(),
        models: models()
            .offered
            .into_iter()
            .map(|model| ModelChoice { model: model.model, label: model.label })
            .collect(),
        efforts: Effort::ALL.to_vec(),
        modes: PermissionMode::CHOSEN.to_vec(),
        defaults: SessionSettings {
            model: ModelName::named("gpt-6.1-sol"),
            permission_mode: PermissionMode::FullAccess,
            effort: Effort::Low,
        },
        features: Vec::new(),
        // The native CLI owns and refreshes this machine's login. It is never read or copied by Roundhouse.
        login: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::model::SessionSettingsChange;

    #[test]
    fn the_catalog_keeps_each_models_efforts_and_defaults() {
        let agent = description();
        assert_eq!(agent.models.len(), 8);
        assert_eq!(agent.defaults.model.as_str(), "gpt-6.1-sol");
        let luna = ModelName::named("gpt-6-luna");
        assert!(!agent.efforts_for(&luna).contains(&Effort::Ultra));
        assert!(agent.efforts_for(&agent.defaults.model).contains(&Effort::Ultra));
        assert_eq!(agent.effort_when_switching(&luna, Effort::Ultra), Effort::Medium);
        assert_eq!(agent.effort_when_switching(&luna, Effort::Low), Effort::Low);
        assert!(
            agent
                .takes(&SessionSettingsChange { model: Some(luna), effort: Some(Effort::Ultra), permission_mode: None })
                .is_err()
        );
        assert_eq!(crate::claude::description().efforts.last(), Some(&Effort::Max));
    }
}
