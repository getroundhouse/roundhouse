use serde_json::json;

use super::*;
use crate::capability::approvals::{ApprovalMode, Unasked};
use crate::capability::settings::{Effort, ModelCatalog, Models, Settings};
use crate::fixtures::offer;

const OPUS: ModelName = ModelName::named("opus");
const HAIKU: ModelName = ModelName::named("haiku");

/// An agent offered Opus at two levels and Haiku at one, starting a chat on Opus at High, with plan mode as well as
/// full access, which compacts when asked.
fn described() -> AgentDescription {
    let mut claims = crate::capability::Capabilities::default();
    claims.approvals.modes = Some(crate::fixtures::modes());
    claims.settings.effort = Some(crate::capability::Claimed {});
    let offered = vec![
        offer("opus", "Opus", &[Effort::Low, Effort::High], Effort::High),
        offer("haiku", "Haiku", &[Effort::Low], Effort::Low),
    ];
    claims.settings.models = Some(Models::Static(ModelCatalog { offered, default_model: Some(OPUS) }));
    claims.context.compaction =
        Some(crate::capability::context::Compaction { keeping: crate::capability::context::Keeping::Read });
    AgentDescription {
        capabilities: claims,
        kind: AgentKind::Claude,
        name: "Agent".to_string(),
        usage_heading: "Agent usage".to_string(),
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
    let haiku_high = Settings { model: Some(HAIKU), effort: Some(Effort::High), ..Settings::default() };
    assert_eq!(agent.takes(&haiku_high), Err(NotOffered::Effort), "a level only another model is offered");
    let asks_first = ApprovalMode { unasked: Unasked::Reads, plan: false, ..plan };
    assert_eq!(agent.takes(&setting(None, Some(asks_first))), Err(NotOffered::Mode), "one it doesn't offer");
    let mut planless = described();
    planless.capabilities.approvals.modes.as_mut().unwrap().others.clear();
    assert_eq!(planless.takes(&setting(None, Some(plan))), Err(NotOffered::Mode));
    assert_eq!(planless.modes(), [crate::fixtures::full_access()]);
    assert_eq!(described().default_mode(), Some(crate::fixtures::full_access()));
    let speedy = Settings { speed: Some(crate::capability::settings::Speed::Fast), ..Settings::default() };
    assert_eq!(agent.takes(&speedy), Err(NotOffered::Settings), "a setting it doesn't claim");
}

#[test]
fn a_level_or_a_mode_from_a_newer_peer_is_never_taken_even_if_offered() {
    let newer_mode = ApprovalMode { unasked: Unasked::Unknown, ..crate::fixtures::full_access() };
    let mut newer = described();
    newer.capabilities.approvals.modes.as_mut().unwrap().others.push(newer_mode);
    let newer_level = offer("opus", "Opus", &[Effort::Unknown], Effort::Unknown);
    newer.capabilities.settings.models =
        Some(Models::Static(ModelCatalog { offered: vec![newer_level], default_model: None }));

    let opus = Settings { model: Some(OPUS), effort: Some(Effort::Unknown), ..Settings::default() };
    assert_eq!(newer.takes(&opus), Err(NotOffered::Effort));
    assert_eq!(newer.takes(&setting(Some(Effort::Unknown), None)), Err(NotOffered::Effort));
    assert_eq!(newer.takes(&setting(None, Some(newer_mode))), Err(NotOffered::Mode));
}

#[test]
fn a_model_offered_is_called_by_its_label_and_any_other_has_none() {
    let agent = described();

    assert_eq!(agent.label(&HAIKU), Some("Haiku"));
    assert_eq!(agent.label(&ModelName::named("claude-opus-5-5")), None);
}

/// A chat starts on the model the claims name, at its own default, in the first mode, or on the first model offered
/// when they name none.
#[test]
fn a_chat_starts_on_the_model_its_claims_name_at_its_default_effort() {
    let agent = described();
    let starts = Settings {
        model: Some(OPUS),
        mode: Some(crate::fixtures::full_access()),
        effort: Some(Effort::High),
        ..Settings::default()
    };
    assert_eq!(agent.defaults(), starts);

    let mut unnamed = described();
    let Some(Models::Static(catalog)) = &mut unnamed.capabilities.settings.models else { panic!("static") };
    catalog.default_model = Some(ModelName::named("not-offered"));
    catalog.offered.reverse();
    assert_eq!((unnamed.defaults().model, unnamed.defaults().effort), (Some(HAIKU), Some(Effort::Low)), "the first");
}

/// One that discovers its models starts a chat only in its first mode until they're said, and then on the catalog's
/// default.
#[test]
fn a_kind_that_discovers_its_models_starts_a_chat_on_the_catalogs_default() {
    let mut discovers = described();
    discovers.capabilities.settings.models = Some(Models::Discoverable);
    assert!(discovers.models().is_empty());
    assert_eq!(discovers.defaults(), Settings { mode: Some(crate::fixtures::full_access()), ..Settings::default() });

    let offered = vec![
        offer("orchard", "Orchard", &[Effort::Low], Effort::Low),
        offer("meadow", "Meadow", &[Effort::Medium], Effort::Medium),
    ];
    let catalog = ModelCatalog { offered, default_model: Some(ModelName::named("meadow")) };
    let there = discovers.with_catalog(catalog);

    assert!(!there.discovers_models());
    assert_eq!(
        (there.defaults().model, there.defaults().effort),
        (Some(ModelName::named("meadow")), Some(Effort::Medium))
    );
}

/// A model named by its full ID takes any level an offered one takes, and starts at the default of the model a chat
/// starts on, since what it takes is the agent's CLI's to say.
#[test]
fn a_model_that_isnt_offered_takes_every_level_an_offered_one_does() {
    let agent = described();
    let full_id = ModelName::named("claude-opus-5-5");

    assert_eq!(agent.efforts_for(&full_id), [Effort::Low, Effort::High]);
    assert_eq!(agent.effort_for_choice(&full_id, None), Effort::High);
    assert_eq!(agent.efforts_for(&HAIKU), [Effort::Low], "an offered one takes its own");
}

#[test]
fn it_compacts_when_asked_only_if_its_claims_say() {
    let mut agent = described();
    assert!(agent.compacts());

    agent.capabilities.context.compaction = None;

    assert!(!agent.compacts());
}

/// What a newer hub adds is in the compatibility tests.
#[test]
fn on_the_wire_it_reads_as_it_was_written() {
    let agent = described();
    let wire = serde_json::to_value(&agent).unwrap();
    assert_eq!(
        wire,
        json!({ "kind": "claude", "name": "Agent", "usage_heading": "Agent usage", "capabilities": wire["capabilities"] })
    );
    assert_eq!(wire["capabilities"]["settings"]["models"]["default_model"], "opus");
    assert_eq!(serde_json::from_value::<AgentDescription>(wire).unwrap(), agent);
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
