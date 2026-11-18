use protocol::capability::settings::{Effort, Settings};
use protocol::model::{AgentDescription, AgentKind, ModelName};

pub fn description() -> AgentDescription {
    AgentDescription {
        kind: AgentKind::Codex,
        capabilities: Some(super::claims::claims()),
        name: "Codex".into(),
        usage_heading: "Codex usage".into(),
        models: Vec::new(),
        efforts: Vec::new(),
        defaults: Settings {
            model: Some(ModelName::named("gpt-6.1-sol")),
            effort: Some(Effort::Low),
            ..Settings::default()
        },
        features: Vec::new(),
        // The native CLI owns and refreshes this machine's login. It is never read or copied by Roundhouse.
        login: None,
    }
}
