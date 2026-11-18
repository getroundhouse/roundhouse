//! Selecting another agent changes only the new chat, and the keyboard keeps the selected chat's agent.

use gpui_kit::{AppContext as _, TestAppContext};
use protocol::capability::settings::Effort;
use protocol::model::{AgentKind, Session};

use crate::support::{Stack, Ui, git};
use crate::tools::in_a_new_workspace;

/// One agent other than Claude, chosen for a new chat: what the chat starts on, what's picked for it after, each with
/// what the chat shows once it's picked, and what it's left on, which the next chat the keyboard opens keeps.
struct Choosing {
    agent: AgentKind,
    item: &'static str,
    starts_on: (&'static str, Effort),
    picked: Vec<Picked>,
    left_on: (&'static str, Effort),
}

/// A model or an effort picked for the chat, and what it then shows: the model, the effort, or both.
struct Picked {
    item: &'static str,
    model: Option<&'static str>,
    effort: Option<Effort>,
}

fn cases() -> [Choosing; 2] {
    let picked = |item, model, effort| Picked { item, model, effort };
    [
        Choosing {
            agent: AgentKind::Codex,
            item: "New Codex chat",
            starts_on: ("orchard", Effort::Low),
            picked: vec![
                picked("deep", None, Effort::parse("deep")),
                // Its supported default effort.
                picked("Meadow", Some("meadow"), Some(Effort::Medium)),
            ],
            left_on: ("meadow", Effort::Medium),
        },
        Choosing {
            agent: AgentKind::Antigravity,
            item: "New Antigravity chat",
            starts_on: ("gemini-4-carbon", Effort::Medium),
            picked: vec![
                picked("Gemini 4 Hydrogen", Some("gemini-4-hydrogen"), None),
                picked("Extra high", None, Some(Effort::Xhigh)),
                picked("Gemini 4 Argon", Some("gemini-4-argon"), Some(Effort::Medium)),
            ],
            left_on: ("gemini-4-argon", Effort::Medium),
        },
    ]
}

/// The agent, model and effort the hub has for a chat.
fn on(session: &Session) -> (AgentKind, &str, Effort) {
    (session.agent, session.model.as_str(), session.effort.clone())
}

#[gpui_kit::test]
fn choosing_another_agent_and_then_using_the_keyboard_keeps_each_chats_agent(cx: &mut TestAppContext) {
    let stack = codex_stack();
    offer_models(&stack);
    let client = stack.client();
    let mut ui = in_a_new_workspace(cx, &stack);
    let claude = ui.active_chat();

    for case in cases() {
        let agent = case.agent;
        let before = (ui.open_chats().len(), ui.active_chat());
        ui.pick_item("new-tab", case.item);
        ui.wait(&format!("{agent:?}: the new chat"), |ui| {
            ui.open_chats().len() == before.0 + 1 && ui.active_chat() != before.1
        });
        let chosen = ui.active_chat();
        let (model, effort) = case.starts_on;
        assert_eq!(on(&client.session_get(&chosen).unwrap()), (agent, model, effort), "{agent:?}: the new chat");
        assert_eq!(client.session_get(&claude).unwrap().agent, AgentKind::Claude, "{agent:?}: the first chat's kept");
        for picked in &case.picked {
            pick(&mut ui, picked, agent);
        }

        ui.press("secondary-t");

        ui.wait(&format!("{agent:?}: another chat"), |ui| {
            ui.open_chats().len() == before.0 + 2 && ui.active_chat() != chosen
        });
        let (model, effort) = case.left_on;
        let next = client.session_get(&ui.active_chat()).unwrap();
        assert_eq!(on(&next), (agent, model, effort), "{agent:?}: the keyboard's chat keeps what was picked");
    }
}

/// Picks a model or an effort for the chat showing, and waits for the chat to show it, since the next shortcut reads
/// the app's session, once the hub's answer has arrived.
fn pick(ui: &mut Ui<'_>, picked: &Picked, agent: AgentKind) {
    let pane = ui.pane().unwrap();
    ui.wait("the chat model catalog", |ui| ui.has("model-picker"));
    ui.click("model-picker");
    ui.click_menu_item(picked.item);
    ui.wait(&format!("{agent:?}: {} shown", picked.item), |ui| {
        ui.cx.read_entity(&pane, |p, _| {
            p.session().is_some_and(|s| {
                picked.model.is_none_or(|model| s.model.as_str() == model)
                    && picked.effort.as_ref().is_none_or(|effort| &s.effort == effort)
            })
        })
    });
}

fn codex_stack() -> Stack {
    let mut environment = crate::support::hub_environment();
    environment.set(agents::ENV_CODEX_BIN, agents::testing::Example::new("native_standin").path());
    Stack::in_environment(&environment)
}

fn offer_models(stack: &Stack) {
    let model = |id, label, default, efforts: &[&str], selected| {
        serde_json::json!({
            "model":id, "displayName":label, "isDefault":default, "hidden":false,
            "supportedReasoningEfforts":efforts.iter().map(|e| serde_json::json!({"reasoningEffort":e})).collect::<Vec<_>>(),
            "defaultReasoningEffort":selected
        })
    };
    let catalog = serde_json::json!({"data":[model("orchard", "Orchard", true, &["low","deep"], "low"),
        model("meadow", "Meadow", false, &["medium"], "medium")]});
    let script = serde_json::json!([{"on":"model/list","actions":[{"result":catalog}]}, {"on":"turn/start","actions":[{"result":{"turn":{"id":"turn"}}}]}]);
    std::fs::write(stack.repo().join("native-script.json"), script.to_string()).unwrap();
    git(&stack.repo(), &["add", "native-script.json"]);
    git(&stack.repo(), &["commit", "-qm", "catalog for this checkout"]);
}

#[gpui_kit::test]
fn discovery_can_be_retried_before_the_first_workspace(cx: &mut TestAppContext) {
    let stack = codex_stack();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = crate::support::Ui::open(cx, &stack);
    ui.wait("the project", |ui| ui.view().records().repos().len() == 1);
    ui.open_new_workspace();
    ui.input("read the project");
    ui.pick_item("new-workspace-agent", "Codex");
    ui.wait("a failed model lookup", |ui| ui.texts_shown().contains("Retry"));
    assert_eq!(ui.dialog_text(), "read the project");
    assert!(client.workspace_list(true).unwrap().is_empty());
    offer_models(&stack);
    ui.click("load-models");
    ui.wait("models from the repaired CLI", |ui| ui.has("new-workspace-model"));
    ui.pick_item("new-workspace-model", "deep");
    ui.click("new-workspace-create");
    ui.wait_for_a_new_workspace(0);
    let session = client.session_get(&ui.active_chat()).unwrap();
    assert_eq!(session.model.as_str(), "orchard");
    assert_eq!(session.effort.as_str(), Some("deep"));
}
