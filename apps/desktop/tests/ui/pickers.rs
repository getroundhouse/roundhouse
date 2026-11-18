//! The model picker, plan mode, and approving a plan.

#![cfg(test)]

use gpui_kit::test::ElementSnapshot;
use gpui_kit::{AppContext as _, ElementId, SharedString, TestAppContext};
use protocol::capability::approvals::ApprovalMode;
use protocol::capability::settings::Effort;
use protocol::event::EventBody;
use protocol::fixtures::{full_access, plan_mode};
use protocol::model::{ModelName, SessionStatus};

use crate::support::{CLAUDE_STARTS_WITH, Stack, Ui};

/// The composer's `+`.
const ADD: &str = "add";

/// Whether the composer shows plan mode, as assistive apps hear it.
fn planning(ui: &mut Ui<'_>) -> bool {
    !ui.labelled("Plan mode").is_empty()
}

#[gpui_kit::test]
fn shift_tab_turns_plan_mode_on_and_off_and_the_model_picker_changes_a_running_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let workspace = ui.new_workspace();
    let chat = ui.active_chat();
    let path = ui.worktree(&workspace);

    // Full access, so a call runs without asking.
    ui.input("tool:echo one > one.txt");
    ui.press("enter");
    ui.wait("the first tool", |ui| ui.session_status() == Some(SessionStatus::Idle) && path.join("one.txt").exists());
    assert!(!planning(&mut ui));

    // Plan mode, on the agent that's already running, asks before a call that writes.
    ui.press("shift-tab");
    ui.wait("plan mode", |ui| ui.chat_mode() == Some(plan_mode()));
    assert!(planning(&mut ui), "the composer shows it's planning");
    assert_eq!(ui.composer_text(), "", "Shift-Tab types nothing");
    ui.input("tool:echo two > two.txt");
    ui.press("enter");
    ui.wait("the question", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    ui.allow_tool_use();
    ui.wait("the second tool", |ui| ui.session_status() == Some(SessionStatus::Idle) && path.join("two.txt").exists());

    // Out of plan mode, and another model, on the same agent.
    ui.press("shift-tab");
    ui.wait("full access", |ui| ui.chat_mode() == Some(full_access()));
    assert!(!planning(&mut ui));
    ui.pick_item("model-picker", "Sonnet");
    ui.wait("the model", |_| client.session_get(&chat).unwrap().model == ModelName::named("sonnet"));
    // An agent started again would be in the chat's events, which come apart from its record. Once the picker's call has
    // ended, whatever it started is in the log ahead of the next turn, so that turn drawn to its end shows it.
    ui.wait_for_every_call();
    ui.input("say:on sonnet");
    ui.press("enter");
    ui.wait_idle_with("a turn on the new model", |text| text.ends_with("on sonnet"));
    let started = ui.events().iter().filter(|e| matches!(e.body, EventBody::AgentStarted { .. })).count();
    assert_eq!(started, 1, "the running agent was changed, not replaced");

    // A new chat keeps the model, but plan mode belongs to the task at hand.
    ui.press("shift-tab");
    ui.wait("plan mode again", |ui| ui.chat_mode() == Some(plan_mode()));
    ui.new_chat();
    assert_eq!(ui.chat_mode(), Some(full_access()));
    assert!(!planning(&mut ui));
    let pane = ui.pane().unwrap();
    assert_eq!(ui.cx.read_entity(&pane, |p, _| p.session().map(|s| s.model.clone())), Some(ModelName::named("sonnet")));
}

/// The item labelled Plan mode in the `+` menu, which is open.
fn plan_item(ui: &mut Ui<'_>) -> ElementSnapshot {
    let menu = ElementId::Name("popup-menu".into());
    let found = ui.labelled("Plan mode").into_iter().find(|found| found.path().contains(&menu));
    found.expect("the menu has plan mode")
}

/// The second of two presses of Shift-Tab undoes the first, even before the agent has answered it. The `+` menu's Plan
/// mode does as Shift-Tab does, is checked while it's on, and leaves focus where the prompt is typed.
#[gpui_kit::test]
fn shift_tab_twice_at_once_leaves_plan_mode_off_and_the_add_menu_turns_it_on_and_off(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    ui.input("say:ready");
    ui.press("enter");
    ui.wait_idle_with("the reply", |text| text == "ready");

    ui.press_at_once(&["shift-tab", "shift-tab"]);

    let changes = |ui: &Ui<'_>| -> Vec<ApprovalMode> {
        ui.events()
            .into_iter()
            .filter_map(|e| match e.body {
                EventBody::AgentMode { mode } => Some(mode),
                _ => None,
            })
            .collect()
    };
    ui.wait("both presses of Shift-Tab answered", |ui| changes(ui).len() == 2);
    assert_eq!(changes(&ui), [plan_mode(), full_access()], "the second press of Shift-Tab undid the first");
    ui.wait("full access after Shift-Tab twice", |ui| ui.chat_mode() == Some(full_access()));
    assert!(!planning(&mut ui), "Shift-Tab twice leaves plan mode off");

    // With nothing focused, so the menu, rather than whatever had focus before it opened, is what puts it in the
    // text field.
    ui.cx.update_window(ui.handle, |_, window, cx| window.blur(cx)).unwrap();
    ui.click(SharedString::from(ADD));
    assert_ne!(plan_item(&mut ui).checked(), Some(true), "the menu's plan mode is off");
    ui.click_menu_item("Plan mode");
    ui.wait("plan mode from the menu", |ui| ui.chat_mode() == Some(plan_mode()));
    assert!(planning(&mut ui), "the composer shows it's planning, from the menu");
    ui.input("next");
    assert_eq!(ui.composer_text(), "next", "focus is where the prompt is typed, after the menu");

    ui.click(SharedString::from(ADD));
    assert_eq!(plan_item(&mut ui).checked(), Some(true), "the menu's plan mode is on");
    ui.click_menu_item("Plan mode");
    ui.wait("full access from the menu", |ui| ui.chat_mode() == Some(full_access()));
    assert!(!planning(&mut ui), "the menu turned plan mode off");
}

#[gpui_kit::test]
fn an_effort_picked_reaches_the_running_agent_and_the_next_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    let chat = ui.active_chat();
    ui.input("effort");
    ui.press("enter");
    ui.wait_idle_with("the hub's default effort", |text| text.contains("effort=xhigh"));

    ui.pick_item("model-picker", "Medium");
    ui.wait("the effort", |_| client.session_get(&chat).unwrap().effort == Effort::Medium);
    ui.input("effort");
    ui.press("enter");
    ui.wait_idle_with("the running agent's new effort", |text| text.contains("effort=medium"));
    let started = ui.events().iter().filter(|e| matches!(e.body, EventBody::AgentStarted { .. })).count();
    assert_eq!(started, 1, "the running agent was changed, not replaced");

    ui.new_chat();
    let pane = ui.pane().unwrap();
    assert_eq!(ui.cx.read_entity(&pane, |p, _| p.session().map(|s| s.effort.clone())), Some(Effort::Medium));
}

#[gpui_kit::test]
fn a_change_the_agent_refuses_is_shown_and_changes_nothing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    ui.input("say:running");
    ui.press("enter");
    ui.wait_idle_with("the agent to be up", |text| text.contains("running"));
    let pane = ui.pane().unwrap();
    ui.cx.update_entity(&pane, |p, cx| p.configure(Some(ModelName::named("not-a-model")), None, cx));
    ui.wait("the refusal", |ui| ui.view().error.as_deref().is_some_and(|e| e.contains("not found")));
    let model = ui.cx.read_entity(&pane, |p, _| p.session().map(|s| s.model.clone()));
    assert_eq!(model, Some(CLAUDE_STARTS_WITH.0));
}

#[gpui_kit::test]
fn a_plan_can_be_approved_or_sent_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    ui.press("shift-tab");
    ui.wait("plan mode", |ui| ui.chat_mode() == Some(plan_mode()));

    ui.input("plan:First attempt");
    ui.press("enter");
    ui.wait("the plan", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    ui.keep_planning();
    ui.wait("the turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle));
    assert_eq!(ui.chat_mode(), Some(plan_mode()), "still planning");

    ui.input("plan:Second attempt");
    ui.press("enter");
    ui.wait("the next plan", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    ui.approve_plan();
    ui.wait("plan mode to end, by the agent's own report", |ui| {
        ui.session_status() == Some(SessionStatus::Idle) && ui.chat_mode() == Some(full_access())
    });
    assert!(!planning(&mut ui));
}
