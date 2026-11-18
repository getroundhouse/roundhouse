//! The first workspace's model choices come from the same CLI as its first turn.

use crate::support::{Stack, git, wait_until};
use protocol::capability::settings::Effort;
use protocol::client::admin::{ModelTarget, ModelsFor};
use protocol::model::{
    AgentKind, ModelListKey, ModelLookup, ModelName, NewWorkspaceDraftToKeep, SessionSettingsChange,
};
use serde_json::{Value, json};

#[test]
fn a_fresh_hub_discovers_the_model_and_effort_it_sends_on_the_first_turn() {
    let program = agents::testing::Example::new("native_standin").path();
    let mut stack = Stack::start_with(vec![(agents::ENV_CODEX_BIN, program.to_string_lossy().into_owned())]);
    stack.make_through_an_adapter();
    let client = stack.client();
    let repo = client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let model = ModelName::named("model-from-this-login");
    offer_models(&stack, &model);

    let asked = ModelsFor { agent: AgentKind::Codex, target: ModelTarget::Project(repo.id.clone()), again: false };
    let first = client.models_for(asked.clone()).unwrap();
    let catalog = answered(&stack, &first.key, 1);
    assert!(client.workspace_list(true).unwrap().is_empty());
    let launch: protocol::provider::OpenPreflight =
        serde_json::from_slice(&std::fs::read(stack.path("instances-made.preflight")).unwrap()).unwrap();
    assert_eq!(launch.project, repo.origin);
    assert_eq!(launch.agent, AgentKind::Codex);
    assert!(stack.instances_made().is_empty());
    let (effort, no_effort) = as_offered(&catalog, &model);
    assert!(!probe(&stack).contains("thread/start") && !probe(&stack).contains("turn/start"));

    let draft = NewWorkspaceDraftToKeep {
        agent: Some(AgentKind::Codex),
        repo_id: Some(repo.id),
        model: Some(model.clone()),
        effort: Some(effort),
        text: "read the project".into(),
        ..Default::default()
    };
    let mut obsolete = draft.clone();
    obsolete.model = Some(ModelName::named("removed-from-the-account"));
    client.new_workspace_draft_keep(&obsolete).unwrap();
    assert!(client.new_workspace_create(&obsolete, "obsolete".into()).is_err());
    assert!(client.workspace_list(true).unwrap().is_empty());
    assert_eq!(client.records_list().unwrap().new_workspace_draft.unwrap().text, obsolete.text);
    let made = client.new_workspace_create(&draft, "discovery".into()).unwrap();
    assert_eq!(made.workspace.provider.as_str(), "recording");
    assert_eq!(made.unchosen, None);
    assert_eq!(made.unsent, None);
    saw_first_turn(&made.workspace, &model);
    let chat = client.models_for(ModelsFor { target: ModelTarget::Session(made.session.id.clone()), ..asked.clone() });
    assert_eq!(chat.unwrap().key, first.key, "the chat's list is the dialog's");
    assert_eq!(lists_asked(&stack), 1, "the dialog's list served the checks and the first chat");
    let obsolete = SessionSettingsChange { model: obsolete.model, ..Default::default() };
    assert!(
        client.session_configure(&made.session.id, obsolete).is_err(),
        "a model no longer offered, from any client"
    );
    let change =
        |effort| SessionSettingsChange { model: Some(no_effort.clone()), effort: Some(effort), permission_mode: None };
    assert!(client.session_configure(&made.session.id, change(Effort::None)).is_err());
    assert_eq!(client.session_get(&made.session.id).unwrap().model, model);
    let cleared = client.session_configure(&made.session.id, change(Effort::Unset)).unwrap();
    assert_eq!(cleared.model, no_effort);
    assert_eq!(cleared.effort, Effort::Unset);

    client.models_for(ModelsFor { again: true, ..asked }).unwrap();
    answered(&stack, &first.key, 2);
}

/// The default effort of `model`, which the catalog offers first, at its efforts, and the model after it, which has none.
fn as_offered(catalog: &protocol::capability::settings::ModelCatalog, model: &ModelName) -> (Effort, ModelName) {
    let [offer, no_effort] = catalog.offered.as_slice() else { panic!("unexpected catalog: {catalog:?}") };
    assert_eq!(catalog.default_model.as_ref(), Some(model));
    assert_eq!(offer.model, *model);
    assert_eq!(offer.efforts[0].id().as_str(), Some("none"));
    assert_eq!(offer.efforts[1].label(), Some("Take time"));
    assert!(no_effort.efforts.is_empty());
    let effort = offer.default_effort.clone().unwrap();
    assert_eq!(effort.as_str(), Some("deliberate"));
    (effort, no_effort.model.clone())
}

/// The catalog of the list at `key`, as every client lists it, once the agent has been asked for it `times` in all,
/// and has answered.
fn answered(stack: &Stack, key: &ModelListKey, times: usize) -> protocol::capability::settings::ModelCatalog {
    let client = stack.client();
    let mut catalog = None;
    wait_until("the agent's answer", || {
        let lists = client.records_list().unwrap().model_lists;
        let list = lists.into_iter().find(|list| list.key == *key && list.lookup == ModelLookup::Answered);
        catalog = list.and_then(|list| list.catalog);
        catalog.is_some() && lists_asked(stack) == times
    });
    catalog.unwrap()
}

/// What the agent's CLI was asked in the project's checkout, where the hub asks it what it offers.
fn probe(stack: &Stack) -> String {
    std::fs::read_to_string(stack.repo().join("native-requests.jsonl")).unwrap_or_default()
}

/// How many times the agent's CLI was asked for its first page of models.
fn lists_asked(stack: &Stack) -> usize {
    let asked = probe(stack);
    let requests = asked.lines().filter_map(|line| serde_json::from_str::<Value>(line).ok());
    requests.filter(|v| v["method"] == "model/list" && v["params"]["cursor"].is_null()).count()
}

fn offer_models(stack: &Stack, model: &ModelName) {
    let other = json!({"model":"without-effort","displayName":"No effort control","isDefault":false,"hidden":false,"supportedReasoningEfforts":[]});
    let native = json!({"model":model,"displayName":"Discovery model","isDefault":true,"hidden":false,
        "supportedReasoningEfforts":[{"reasoningEffort":"none"},{"reasoningEffort":"deliberate","displayName":"Take time"}],"defaultReasoningEffort":"deliberate"});
    let script = json!([
        {"on":"model/list","params":{"cursor":null,"includeHidden":false},"actions":[{"result":{"data":[native],"nextCursor":"page-two"}}]},
        {"on":"model/list","params":{"cursor":"page-two","includeHidden":false},"actions":[{"result":{"data":[other],"nextCursor":null}}]},
        {"on":"turn/start","actions":[{"result":{"turn":{"id":"turn"}}}]}
    ]);
    std::fs::write(stack.repo().join("native-script.json"), script.to_string()).unwrap();
    git(&stack.repo(), &["add", "native-script.json"]);
    git(&stack.repo(), &["commit", "-qm", "the project's model source"]);
}

fn saw_first_turn(workspace: &protocol::model::Workspace, model: &ModelName) {
    let requests = std::path::Path::new(workspace.path.as_ref().unwrap()).join("native-requests.jsonl");
    wait_until("first turn with the selected model and effort", || {
        std::fs::read_to_string(&requests)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .any(|v| {
                v["method"] == "turn/start"
                    && v["params"]["model"] == model.as_str()
                    && v["params"]["effort"] == "deliberate"
            })
    });
}
