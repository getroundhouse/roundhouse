//! Selecting another agent changes only the new chat, and the keyboard keeps the selected chat's agent.

use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::{AgentKind, Effort};

use crate::support::Stack;
use crate::tools::in_a_new_workspace;

#[gpui_kit::test]
fn choosing_codex_and_then_using_the_keyboard_keeps_each_chats_agent(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    let mut ui = in_a_new_workspace(cx, &stack);
    let claude = ui.active_chat();

    ui.pick_item("new-tab", "New Codex chat");
    ui.wait("the Codex chat", |ui| ui.open_chats().len() == 2 && ui.active_chat() != claude);
    let codex = ui.active_chat();
    let chosen = client.session_get(&codex).unwrap();
    assert_eq!(chosen.agent, AgentKind::Codex);
    assert_eq!(chosen.model.as_str(), "gpt-6.1-sol");
    assert_eq!(chosen.effort, Effort::Low);
    assert_eq!(client.session_get(&claude).unwrap().agent, AgentKind::Claude);
    let pane = ui.pane().unwrap();

    ui.click("model-picker");
    ui.click_menu_item("Ultra");
    ui.wait("Ultra shown on Sol", |ui| {
        ui.cx.read_entity(&pane, |p, _| p.session().is_some_and(|s| s.effort == Effort::Ultra))
    });
    ui.click("model-picker");
    ui.click_menu_item("GPT-6 Luna");
    // The next shortcut reads the app's session, after its reply/feed has arrived from the hub.
    ui.wait("Luna and its supported default effort shown", |ui| {
        ui.cx.read_entity(&pane, |p, _| {
            p.session().is_some_and(|s| s.model.as_str() == "gpt-6-luna" && s.effort == Effort::Medium)
        })
    });

    ui.press("secondary-t");
    ui.wait("another Codex chat", |ui| ui.open_chats().len() == 3 && ui.active_chat() != codex);

    let next = client.session_get(&ui.active_chat()).unwrap();
    assert_eq!(next.agent, AgentKind::Codex);
    assert_eq!(next.model.as_str(), "gpt-6-luna");
    assert_eq!(next.effort, Effort::Medium);
}

#[gpui_kit::test]
fn choosing_antigravity_and_then_using_the_keyboard_keeps_each_chats_agent(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    let mut ui = in_a_new_workspace(cx, &stack);
    let claude = ui.active_chat();

    ui.pick_item("new-tab", "New Antigravity chat");
    ui.wait("the Antigravity chat", |ui| ui.open_chats().len() == 2 && ui.active_chat() != claude);
    let agy = ui.active_chat();
    let chosen = client.session_get(&agy).unwrap();
    assert_eq!(chosen.agent, AgentKind::Antigravity);
    assert_eq!(chosen.model.as_str(), "gemini-4-carbon");
    assert_eq!(chosen.effort, Effort::Medium);
    assert_eq!(client.session_get(&claude).unwrap().agent, AgentKind::Claude);
    let pane = ui.pane().unwrap();

    ui.click("model-picker");
    ui.click_menu_item("Gemini 4 Hydrogen");
    ui.wait("Hydrogen shown", |ui| {
        ui.cx.read_entity(&pane, |p, _| p.session().is_some_and(|s| s.model.as_str() == "gemini-4-hydrogen"))
    });
    ui.click("model-picker");
    ui.click_menu_item("Extra high");
    ui.wait("Extra high shown on Hydrogen", |ui| {
        ui.cx.read_entity(&pane, |p, _| p.session().is_some_and(|s| s.effort == Effort::Xhigh))
    });
    ui.click("model-picker");
    ui.click_menu_item("Gemini 4 Argon");
    ui.wait("Argon and its supported default effort shown", |ui| {
        ui.cx.read_entity(&pane, |p, _| {
            p.session().is_some_and(|s| s.model.as_str() == "gemini-4-argon" && s.effort == Effort::Medium)
        })
    });

    ui.press("secondary-t");
    ui.wait("another Antigravity chat", |ui| ui.open_chats().len() == 3 && ui.active_chat() != agy);

    let next = client.session_get(&ui.active_chat()).unwrap();
    assert_eq!(next.agent, AgentKind::Antigravity);
    assert_eq!(next.model.as_str(), "gemini-4-argon");
    assert_eq!(next.effort, Effort::Medium);
}
