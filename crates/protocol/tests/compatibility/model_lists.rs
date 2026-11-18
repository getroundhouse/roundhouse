//! Every message about a list of models, spelled out, since a client and its hub can each be a different build, and
//! an agent's description says whether its models are discovered in a form a client from before reads.

use protocol::capability::settings::{EffortOffer, ModelCatalog, ModelOffer, Models, Stage};
use protocol::client::{ClientResponse, HubFrame, HubNotice, ReplyOutcome};
use protocol::model::{Effort, ModelList, ModelListKey, ModelLookup, ModelName, RecordKey, Records, Rev};

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

fn list() -> ModelList {
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
    let catalog = ModelCatalog { offered: vec![offer], default_model: Some(ModelName::named("orchard")) };
    ModelList { key: key(), catalog: Some(catalog), lookup: ModelLookup::Answered, rev: Rev::new(9) }
}

const KEY: &str = r#""agent":"codex","repo":"r","provider":"local","account":null"#;
const CATALOG: &str = r#""catalog":{"offered":[{"model":"orchard","label":"Orchard","efforts":["low",{"id":"deep","label":"Deep"}],"default_effort":"low","speeds":[],"inputs":[],"stage":"current"}],"default_model":"orchard"}"#;

#[test]
fn a_list_keeps_its_form_as_a_reply_a_notice_and_a_removal() {
    let record = format!(r#"{KEY},{CATALOG},"lookup":{{"state":"answered"}},"rev":9"#);

    let reply = HubFrame::Reply { id: 7, outcome: ReplyOutcome::Response(ClientResponse::ModelList(list())) };
    keeps_its_form(&reply, &format!(r#"{{"frame":"reply","id":7,"response":{{"result":"model_list",{record}}}}}"#));
    keeps_its_form(&HubNotice::ModelList(list()), &format!(r#"{{"notice":"model_list",{record}}}"#));
    let removed = HubNotice::Removed { record: RecordKey::ModelList(key()), rev: Rev::new(10) };
    keeps_its_form(&removed, &format!(r#"{{"notice":"removed","record":{{"kind":"model_list",{KEY}}},"rev":10}}"#));
    let failed = ModelList { catalog: None, lookup: ModelLookup::Failed { message: "logged out".into() }, ..list() };
    keeps_its_form(
        &failed,
        &format!(r#"{{{KEY},"catalog":null,"lookup":{{"state":"failed","message":"logged out"}},"rev":9}}"#),
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
    keeps_its_form(&Models::Static { offered: Vec::new() }, r#"{"offered":[]}"#);
}
