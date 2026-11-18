use protocol::model::{AgentDescription, AgentKind, Effort, ModelChoice, ModelName, PermissionMode, SessionSettings};

// A compiled snapshot of `agy` 1.5.9's native catalog: the Gemini models it offers under both Google OAuth and
// `gemini-api-key`, then the Claude models it offers only under Google OAuth. Refresh it with the CLI observations in
// docs/antigravity-cli.md.
const MODELS: [(&str, &str, &[Effort], Effort); 6] = [
    ("gemini-4-carbon", "Gemini 4 Carbon", &[Effort::Low, Effort::Medium, Effort::High], Effort::Medium),
    (
        "gemini-4-hydrogen",
        "Gemini 4 Hydrogen",
        &[Effort::Low, Effort::Medium, Effort::High, Effort::Xhigh],
        Effort::High,
    ),
    ("gemini-4-argon", "Gemini 4 Argon", &[Effort::Low, Effort::Medium, Effort::High], Effort::Medium),
    ("gemini-3.8-flash", "Gemini 3.8 Flash", &[Effort::Low, Effort::Medium, Effort::High], Effort::Low),
    // `agy` doesn't say a Claude model's default thinking level, so each takes the middle one.
    ("claude-opus-5-5", "Claude Opus 5.5", &[Effort::Low, Effort::Medium, Effort::High], Effort::Medium),
    ("claude-sonnet-5-5", "Claude Sonnet 5.5", &[Effort::Low, Effort::Medium, Effort::High], Effort::Medium),
];

pub(super) const TRIAL_MODEL: &str = "gemini-3.8-flash";

pub(super) fn models() -> protocol::capability::settings::Models {
    use protocol::capability::settings::{Input, ModelOffer, Models, Stage};
    Models::Static {
        offered: MODELS
            .into_iter()
            .map(|(model, label, efforts, default)| ModelOffer {
                model: ModelName::named(model),
                label: label.into(),
                description: None,
                model_provider: Some("Google".into()),
                efforts: efforts.iter().cloned().map(protocol::capability::settings::EffortOffer::Id).collect(),
                default_effort: Some(default.clone()),
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
        kind: AgentKind::Antigravity,
        capabilities: Some(super::claims::claims()),
        name: "Antigravity".into(),
        usage_heading: "Antigravity usage".into(),
        models: models()
            .offered()
            .iter()
            .cloned()
            .map(|model| ModelChoice { model: model.model, label: model.label })
            .collect(),
        efforts: vec![Effort::Low, Effort::Medium, Effort::High, Effort::Xhigh],
        modes: PermissionMode::CHOSEN.to_vec(),
        defaults: SessionSettings {
            model: ModelName::named("gemini-4-carbon"),
            permission_mode: PermissionMode::FullAccess,
            effort: Effort::Medium,
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
        assert_eq!(agent.defaults.model.as_str(), "gemini-4-carbon");
        assert_eq!(agent.defaults.effort, Effort::Medium);
        let hydrogen = ModelName::named("gemini-4-hydrogen");
        let carbon = ModelName::named("gemini-4-carbon");
        let opus = ModelName::named("claude-opus-5-5");
        let sonnet = ModelName::named("claude-sonnet-5-5");
        assert!(agent.efforts_for(&hydrogen).contains(&Effort::Xhigh));
        assert!(!agent.efforts_for(&carbon).contains(&Effort::Xhigh));
        assert_eq!(agent.efforts_for(&opus), [Effort::Low, Effort::Medium, Effort::High]);
        assert_eq!(agent.efforts_for(&sonnet), [Effort::Low, Effort::Medium, Effort::High]);
        assert_eq!(agent.effort_for_choice(&carbon, Some(&Effort::Xhigh)), Effort::Medium);
        assert_eq!(agent.effort_for_choice(&opus, Some(&Effort::Xhigh)), Effort::Medium);
        assert_eq!(agent.effort_for_choice(&sonnet, Some(&Effort::High)), Effort::High);
        assert_eq!(agent.effort_for_choice(&hydrogen, Some(&Effort::Low)), Effort::Low);
        assert!(
            agent
                .takes(&SessionSettingsChange {
                    model: Some(carbon),
                    effort: Some(Effort::Xhigh),
                    permission_mode: None,
                })
                .is_err()
        );
    }
}
