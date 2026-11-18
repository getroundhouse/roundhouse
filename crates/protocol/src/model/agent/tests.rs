use serde_json::json;

use super::*;
use crate::capability::approvals::{ApprovalMode, Unasked};

const OPUS: ModelName = ModelName::named("opus");

/// An agent offered two models, two levels and plan mode as well as full access, which compacts when asked.
fn described() -> AgentDescription {
    let mut claims = crate::capability::Capabilities::default();
    claims.approvals.modes = Some(crate::fixtures::modes());
    claims.settings.effort = Some(crate::capability::Claimed {});
    AgentDescription {
        capabilities: Some(claims),
        kind: AgentKind::Claude,
        name: "Agent".to_string(),
        usage_heading: "Agent usage".to_string(),
        models: vec![
            ModelChoice { model: OPUS, label: "Opus".to_string() },
            ModelChoice { model: ModelName::named("haiku"), label: "Haiku".to_string() },
        ],
        efforts: vec![Effort::Low, Effort::High],
        defaults: Settings { model: Some(OPUS), effort: Some(Effort::High), ..Settings::default() },
        features: vec![AgentFeature::Compacts],
        login: None,
    }
}

fn setting(effort: Option<Effort>, mode: Option<ApprovalMode>) -> Settings {
    Settings { model: Some(ModelName::named("claude-opus-5-5")), mode, effort, ..Settings::default() }
}

#[test]
fn a_chat_is_given_only_an_effort_and_a_mode_its_agent_is_offered_and_any_model() {
    let agent = described();
    let plan = crate::fixtures::plan_mode();

    assert_eq!(agent.takes(&setting(Some(Effort::High), Some(plan))), Ok(()));
    assert_eq!(agent.takes(&Settings::default()), Ok(()), "nothing to change");
    assert_eq!(agent.takes(&setting(Some(Effort::Max), None)), Err(NotOffered::Effort));
    assert_eq!(agent.takes(&setting(Some(Effort::Unknown), None)), Err(NotOffered::Effort));
    let asks_first = ApprovalMode { unasked: Unasked::Reads, plan: false, ..plan };
    assert_eq!(agent.takes(&setting(None, Some(asks_first))), Err(NotOffered::Mode), "one it doesn't offer");
    let mut planless = described();
    planless.capabilities.as_mut().unwrap().approvals.modes.as_mut().unwrap().others.clear();
    assert_eq!(planless.takes(&setting(None, Some(plan))), Err(NotOffered::Mode));
    assert_eq!(planless.modes(), [crate::fixtures::full_access()]);
    assert_eq!(described().default_mode(), Some(crate::fixtures::full_access()));
    let speedy = Settings { speed: Some(crate::capability::settings::Speed::Fast), ..Settings::default() };
    assert_eq!(agent.takes(&speedy), Err(NotOffered::Settings), "a setting it doesn't claim");
}

#[test]
fn a_level_or_a_mode_from_a_newer_peer_is_never_taken_even_if_offered() {
    let newer_mode = ApprovalMode { unasked: Unasked::Unknown, ..crate::fixtures::full_access() };
    let mut newer = AgentDescription { efforts: vec![Effort::Unknown], ..described() };
    newer.capabilities.as_mut().unwrap().approvals.modes.as_mut().unwrap().others.push(newer_mode);

    assert_eq!(newer.takes(&setting(Some(Effort::Unknown), None)), Err(NotOffered::Effort));
    assert_eq!(newer.takes(&setting(None, Some(newer_mode))), Err(NotOffered::Mode));
}

#[test]
fn a_model_offered_is_called_by_its_label_and_any_other_has_none() {
    let agent = described();

    assert_eq!(agent.label(&ModelName::named("haiku")), Some("Haiku"));
    assert_eq!(agent.label(&ModelName::named("claude-opus-5-5")), None);
}

/// What a newer hub adds is in the compatibility tests.
#[test]
fn on_the_wire_it_reads_as_it_was_written() {
    let agent = described();
    let wire = serde_json::to_value(&agent).unwrap();
    assert_eq!(
        wire,
        json!({
            "kind": "claude", "name": "Agent", "usage_heading": "Agent usage",
            "models": [{ "model": "opus", "label": "Opus" }, { "model": "haiku", "label": "Haiku" }],
            "capabilities": wire["capabilities"],
            "efforts": ["low", "high"],
            "defaults": { "model": "opus", "mode": null, "effort": "high", "speed": null, "thinking": null,
                "persona": null, "style": null, "verbosity": null, "advisor": null, "budget": null },
            "features": ["compacts"],
        })
    );
    assert_eq!(serde_json::from_value::<AgentDescription>(wire).unwrap(), agent);
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
