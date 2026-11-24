//! Every message about a list of models, spelled out, since a client and its hub can each be a different build, and
//! an agent's description says whether its models are discovered in a form a client from before reads.

use protocol::capability::settings::Effort;
use protocol::capability::settings::{EffortOffer, ModelCatalog, ModelOffer, Models, Stage};
use protocol::client::{ClientResponse, HubFrame, HubNotice, ReplyOutcome};
use protocol::model::{
    ModelList, ModelListKey, ModelLookup, ModelName, Observation, ObservationKey, Observed, RecordKey, Records, Rev,
};

use crate::keeps_its_form;

fn key() -> ModelListKey {
    ModelListKey {
        agent: protocol::model::AgentKind::Codex,
        repo: "r".into(),
        provider: "local".into(),
        account: None,
        cli_version: None,
    }
}

/// What Codex offers, as one model.
fn catalog() -> ModelCatalog {
    let offer = ModelOffer {
        model: ModelName::named("orchard"),
        label: "Orchard".into(),
        description: None,
        model_provider: None,
        efforts: vec![
            EffortOffer::Id(Effort::Low),
            EffortOffer::Named { id: Effort::parse("deep").unwrap(), label: "Deep".into() },
        ],
        default_effort: Some(Effort::Low),
        speeds: Vec::new(),
        inputs: Vec::new(),
        context_tokens: None,
        price: None,
        stage: Stage::Current,
    };
    ModelCatalog { offered: vec![offer], default_model: Some(ModelName::named("orchard")) }
}

fn list() -> ModelList {
    ModelList { key: key(), lookup: ModelLookup::Answered, rev: Rev::new(9) }
}

const KEY: &str = r#""agent":"codex","repo":"r","provider":"local","account":null"#;
const CATALOG: &str = r#""catalog":{"offered":[{"model":"orchard","label":"Orchard","efforts":["low",{"id":"deep","label":"Deep"}],"default_effort":"low","speeds":[],"inputs":[],"stage":"current"}],"default_model":"orchard"}"#;

#[test]
fn a_list_keeps_its_form_as_a_reply_a_notice_and_a_removal() {
    let record = format!(r#"{KEY},"lookup":{{"state":"answered"}},"rev":9"#);
    let filled = format!(r#"{{"notice":"model_list",{KEY},{CATALOG},"lookup":{{"state":"answered"}},"rev":9}}"#);

    let reply = HubFrame::Reply { id: 7, outcome: ReplyOutcome::Response(ClientResponse::ModelList(list())) };
    keeps_its_form(&reply, &format!(r#"{{"frame":"reply","id":7,"response":{{"result":"model_list",{record}}}}}"#));
    keeps_its_form(&HubNotice::ModelList(list()), &format!(r#"{{"notice":"model_list",{record}}}"#));
    let removed = HubNotice::Removed { record: RecordKey::ModelList(key()), rev: Rev::new(10) };
    keeps_its_form(&removed, &format!(r#"{{"notice":"removed","record":{{"kind":"model_list",{KEY}}},"rev":10}}"#));
    let failed = ModelList { lookup: ModelLookup::Failed { message: "logged out".into() }, ..list() };
    keeps_its_form(&failed, &format!(r#"{{{KEY},"lookup":{{"state":"failed","message":"logged out"}},"rev":9}}"#));
    // As the hub of the build before sends it, with what the agent offered on it for a client of version 4.
    assert_eq!(serde_json::from_str::<HubNotice>(&filled).unwrap(), HubNotice::ModelList(list()));
}

/// What an agent offers, which a hub sends at its own level to a client of a version that reads it so, as a notice and
/// as its removal, keyed as its list is.
#[test]
fn a_catalog_keeps_its_form_as_an_observation_and_its_removal() {
    let catalog = Observed::AsOf { value: catalog(), as_of_ms: 1_795_230_000_000 };
    let copy = HubNotice::Observation(Observation::ModelCatalog { list: key(), catalog, rev: Rev::new(5) });
    let gone =
        HubNotice::ObservationRemoved { observation: ObservationKey::ModelCatalog { list: key() }, rev: Rev::new(6) };
    let catalog = &CATALOG["\"catalog\":".len()..];

    keeps_its_form(
        &copy,
        &format!(
            r#"{{"notice":"observation","kind":"model_catalog","list":{{{KEY}}},"catalog":{{"freshness":"as_of","value":{catalog},"as_of_ms":1795230000000}},"rev":5}}"#
        ),
    );
    keeps_its_form(
        &gone,
        &format!(
            r#"{{"notice":"observation_removed","observation":{{"kind":"model_catalog","list":{{{KEY}}}}},"rev":6}}"#
        ),
    );
}

/// A hub from before them lists none, and a newer hub's state of a lookup reads as unknown rather than failing the list.
#[test]
fn a_list_of_records_reads_without_them_and_a_newer_state_reads_as_unknown() {
    let before: Records =
        serde_json::from_str(r#"{"repos":[],"workspaces":[],"sessions":[],"comments":[],"rev":4}"#).unwrap();
    let newer: ModelList =
        serde_json::from_str(&format!(r#"{{{KEY},"catalog":null,"lookup":{{"state":"queued"}},"rev":9}}"#)).unwrap();

    assert_eq!(before.model_lists, Vec::new());
    assert!(!serde_json::to_string(&before).unwrap().contains("model_lists"));
    assert_eq!(newer.lookup, ModelLookup::Unknown);
}

/// A client from before reads the offered models of both forms, which is none before they're discovered.
#[test]
fn an_agents_models_say_whether_theyre_discovered_in_a_form_a_client_from_before_reads() {
    keeps_its_form(&Models::Discoverable, r#"{"discoverable":true,"offered":[]}"#);
    keeps_its_form(&Models::Static(ModelCatalog::default()), r#"{"offered":[]}"#);
    let starting = ModelCatalog { offered: Vec::new(), default_model: Some(ModelName::named("opus")) };
    keeps_its_form(&Models::Static(starting), r#"{"offered":[],"default_model":"opus"}"#);
}
