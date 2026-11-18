use protocol::model::{AgentDescription, AgentKind, Effort, ModelName, PermissionMode, SessionSettings};

pub fn description() -> AgentDescription {
    AgentDescription {
        kind: AgentKind::Codex,
        capabilities: Some(super::claims::claims()),
        name: "Codex".into(),
        usage_heading: "Codex usage".into(),
        models: Vec::new(),
        efforts: Vec::new(),
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
