use serde_json::json;

use super::*;

const OPUS: ModelName = ModelName::named("opus");

/// An agent offered two models, two levels and plan mode as well as full access, which compacts when asked.
fn described() -> AgentDescription {
    AgentDescription {
        capabilities: None,
        kind: AgentKind::Claude,
        name: "Agent".to_string(),
        usage_heading: "Agent usage".to_string(),
        models: vec![
            ModelChoice { model: OPUS, label: "Opus".to_string() },
            ModelChoice { model: ModelName::named("haiku"), label: "Haiku".to_string() },
        ],
        efforts: vec![Effort::Low, Effort::High],
        modes: vec![PermissionMode::FullAccess, PermissionMode::Plan],
        defaults: SessionSettings { model: OPUS, permission_mode: PermissionMode::FullAccess, effort: Effort::High },
        features: vec![AgentFeature::Compacts],
        login: None,
    }
}

fn setting(effort: Option<Effort>, permission_mode: Option<PermissionMode>) -> SessionSettingsChange {
    SessionSettingsChange { model: Some(ModelName::named("claude-opus-5-5")), permission_mode, effort }
}

#[test]
fn a_chat_is_given_only_an_effort_and_a_mode_its_agent_is_offered_and_any_model() {
    let agent = described();

    assert_eq!(agent.takes(&setting(Some(Effort::High), Some(PermissionMode::Plan))), Ok(()));
    assert_eq!(agent.takes(&SessionSettingsChange::default()), Ok(()), "nothing to change");
    assert_eq!(agent.takes(&setting(Some(Effort::Max), None)), Err(NotOffered::Effort));
    assert_eq!(agent.takes(&setting(Some(Effort::Unknown), None)), Err(NotOffered::Effort));
    assert_eq!(agent.takes(&setting(None, Some(PermissionMode::Unknown))), Err(NotOffered::Mode));
    let planless = AgentDescription { modes: vec![PermissionMode::FullAccess], ..described() };
    assert_eq!(planless.takes(&setting(None, Some(PermissionMode::Plan))), Err(NotOffered::Mode));
}

#[test]
fn a_level_or_a_mode_from_a_newer_peer_is_never_taken_even_if_offered() {
    let newer =
        AgentDescription { efforts: vec![Effort::Unknown], modes: vec![PermissionMode::Unknown], ..described() };

    assert_eq!(newer.takes(&setting(Some(Effort::Unknown), None)), Err(NotOffered::Effort));
    assert_eq!(newer.takes(&setting(None, Some(PermissionMode::Unknown))), Err(NotOffered::Mode));
}

#[test]
fn a_model_offered_is_called_by_its_label_and_any_other_has_none() {
    let agent = described();

    assert_eq!(agent.label(&ModelName::named("haiku")), Some("Haiku"));
    assert_eq!(agent.label(&ModelName::named("claude-opus-5-5")), None);
}

#[test]
fn on_the_wire_it_reads_as_it_was_written_and_a_feature_from_a_newer_hub_is_unknown() {
    let agent = described();
    let wire = serde_json::to_value(&agent).unwrap();
    assert_eq!(
        wire,
        json!({
            "kind": "claude", "name": "Agent", "usage_heading": "Agent usage",
            "models": [{ "model": "opus", "label": "Opus" }, { "model": "haiku", "label": "Haiku" }],
            "efforts": ["low", "high"], "modes": ["bypassPermissions", "plan"],
            "defaults": { "model": "opus", "permission_mode": "bypassPermissions", "effort": "high" },
            "features": ["compacts"],
        })
    );
    assert_eq!(serde_json::from_value::<AgentDescription>(wire.clone()).unwrap(), agent);

    let mut newer = wire;
    newer["features"] = json!(["compacts", "flies"]);
    newer["efforts"] = json!(["low", "future"]);
    let read: AgentDescription = serde_json::from_value(newer).unwrap();
    assert_eq!(read.features, [AgentFeature::Compacts, AgentFeature::Unknown]);
    assert_eq!(read.efforts, [Effort::Low, Effort::Unknown]);
}

#[test]
fn a_feature_left_out_is_one_the_agent_doesnt_have() {
    let mut wire = serde_json::to_value(described()).unwrap();
    wire.as_object_mut().unwrap().remove("features");

    let read: AgentDescription = serde_json::from_value(wire).unwrap();

    assert!(!read.has(AgentFeature::Compacts));
}

#[test]
fn a_login_reads_as_it_was_written_and_one_that_doesnt_say_how_long_it_lasts_still_reads() {
    let login = LoginDescription { made_with: "agent make-token".to_string(), lasts_ms: Some(86_400_000) };
    let agent = AgentDescription { login: Some(login.clone()), ..described() };

    let wire = serde_json::to_value(&agent).unwrap();
    assert_eq!(wire["login"], json!({ "made_with": "agent make-token", "lasts_ms": 86_400_000 }));
    assert_eq!(serde_json::from_value::<AgentDescription>(wire).unwrap().login, Some(login));

    let unsaid: LoginDescription = serde_json::from_value(json!({ "made_with": "agent make-token" })).unwrap();
    assert_eq!(unsaid.lasts_ms, None);
}
