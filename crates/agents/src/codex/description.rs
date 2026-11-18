use protocol::model::{AgentDescription, AgentKind};

pub fn description() -> AgentDescription {
    AgentDescription {
        kind: AgentKind::Codex,
        // Its models are discovered where a chat would launch, so it names none, nor one a chat starts on.
        capabilities: super::claims::claims(),
        name: "Codex".into(),
        usage_heading: "Codex usage".into(),
        // The native CLI owns and refreshes this machine's login. It is never read or copied by Roundhouse.
        login: None,
    }
}
