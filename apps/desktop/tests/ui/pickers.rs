//! The model and approval mode pickers, and approving a plan.

#![cfg(test)]

use gpui_kit::{AppContext as _, ElementId, TestAppContext};
use protocol::event::EventBody;
use protocol::model::SessionStatus;

use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn the_pickers_change_a_running_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let workspace = ui.new_workspace();
    let chat = ui.active_chat();
    let path = ui.worktree(&workspace);

    // Ask first, chosen before anything has run, means the first launch asks.
    ui.pick("mode-picker", 0);
    ui.wait("the mode to change", |ui| ui.chat_mode().as_deref() == Some("manual"));
    ui.input("tool:echo one > one.txt");
    ui.press("enter");
    ui.wait("the question", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    let seq = ui.next_open_approval();
    ui.click(ElementId::NamedInteger("allow".into(), seq));
    ui.wait("the first tool", |ui| ui.session_status() == Some(SessionStatus::Idle) && path.join("one.txt").exists());

    // Full access and another model, on the agent that's already running.
    ui.pick("mode-picker", 3);
    ui.wait("full access", |ui| ui.chat_mode().as_deref() == Some("bypassPermissions"));
    ui.pick("model-picker", 2);
    ui.wait("the model", |_| client.session_get(&chat).unwrap().model.as_deref() == Some("opus"));
    ui.input("tool:echo two > two.txt");
    ui.press("enter");
    ui.wait("the second tool, without asking", |ui| {
        ui.session_status() == Some(SessionStatus::Idle) && path.join("two.txt").exists()
    });
    let started = ui.events().iter().filter(|e| matches!(e.body, EventBody::AgentStarted { .. })).count();
    assert_eq!(started, 1, "the running agent was changed, not replaced");

    // A new chat doesn't quietly get full access.
    ui.new_chat();
    assert_eq!(ui.chat_mode(), None);
    let pane = ui.pane().unwrap();
    assert_eq!(ui.cx.read_entity(&pane, |p, _| p.session().and_then(|s| s.model.clone())).as_deref(), Some("opus"));
}

#[gpui_kit::test]
fn a_change_the_agent_refuses_is_shown_and_changes_nothing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input("say:running");
    ui.press("enter");
    ui.wait_idle_with("the agent to be up", |text| text.contains("running"));
    let pane = ui.pane().unwrap();
    ui.cx.update_entity(&pane, |p, cx| p.configure(Some("not-a-model".to_string()), None, cx));
    ui.wait("the refusal", |ui| ui.view().error.as_deref().is_some_and(|e| e.contains("not found")));
    assert_eq!(ui.cx.read_entity(&pane, |p, _| p.session().and_then(|s| s.model.clone())), None);
}

#[gpui_kit::test]
fn a_plan_can_be_approved_or_sent_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.pick("mode-picker", 1);
    ui.wait("plan mode", |ui| ui.chat_mode().as_deref() == Some("plan"));

    ui.input("plan:First attempt");
    ui.press("enter");
    ui.wait("the plan", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    let seq = ui.next_open_approval();
    ui.click(ElementId::NamedInteger("keep-planning".into(), seq));
    ui.wait("the turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));
    assert_eq!(ui.chat_mode().as_deref(), Some("plan"), "still planning");

    ui.input("plan:Second attempt");
    ui.press("enter");
    ui.wait("the next plan", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    let seq = ui.next_open_approval();
    ui.click(ElementId::NamedInteger("approve-plan".into(), seq));
    ui.wait("plan mode to end, by the agent's own report", |ui| {
        ui.session_status() == Some(SessionStatus::Idle) && ui.chat_mode().as_deref() == Some("manual")
    });
}
