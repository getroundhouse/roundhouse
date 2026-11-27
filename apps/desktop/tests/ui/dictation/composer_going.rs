//! Dictating ends once its composer isn't drawn, dropping what wasn't final: for Settings, another chat, a question,
//! its workspace being archived or the hub being lost. Another app coming in front only stops it, as letting go of the
//! key does, and the hub restarting, which leaves the composer drawn, changes nothing.

use desktop::app::Linking;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::SessionStatus;

use super::support::{ended, hearing, hold_option_space, kept, live, on_the_keyboard, press_space, ready, say};
use crate::support::{Stack, Ui, id};

#[gpui_kit::test]
fn opening_settings_ends_dictation_and_drops_what_was_not_final(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.click("dictate");
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");

    ui.press("secondary-,");
    ui.press("escape");

    assert_eq!(live(&mut ui), None);
    ui.click("dictate");
    say(&mut ui, &engine, "in hub");
    ui.click(dictation_ui::STOP);
    ui.wait("only what's said since", |ui| ui.composer_text() == "In hub.");
}

#[gpui_kit::test]
fn another_chat_ends_dictation_and_nothing_goes_in_either(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let first = ui.active_chat();
    let engine = ready(&mut ui);
    ui.click("dictate");
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");

    ui.new_chat();

    assert_eq!(live(&mut ui), None);
    ui.click("dictate");
    say(&mut ui, &engine, "in hub");
    ui.click(dictation_ui::STOP);
    ui.wait("only what's said in this chat", |ui| ui.composer_text() == "In hub.");
    assert_eq!(kept(&stack, &first), "");
}

#[gpui_kit::test]
fn a_question_taking_the_composers_place_ends_dictation(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    let engine = ready(&mut ui);
    ui.click("dictate");
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");

    // From another client, so the composer goes on being dictated to until the question comes.
    stack.client().session_prompt(&chat, "ask:Red or blue?=Red/Blue", None).unwrap();
    ui.wait("the agent to ask", |ui| ui.session_status() == Some(SessionStatus::WaitingApproval));
    ui.next_open_approval();
    ui.wait("what it asks to show", |ui| ui.has(id("question")));
    ui.click(id("question-answer-0"));
    ui.click(id("question-send"));
    ui.wait_for_the_turn();

    assert_eq!(live(&mut ui), None, "the composer comes back with nothing being dictated");
    assert_eq!(ui.composer_text(), "");
}

#[gpui_kit::test]
fn archiving_the_workspace_ends_dictation_and_another_can_be_dictated_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let first = ui.new_workspace();
    let engine = ready(&mut ui);
    ui.click("dictate");
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");

    stack.client().workspace_archive(&first, false).unwrap();
    ui.wait("its pane to go", |ui| !ui.view().panes.contains_key(&first));

    ui.new_workspace();
    ui.click("dictate");
    say(&mut ui, &engine, "in hub");
    ui.click(dictation_ui::STOP);
    ui.wait("what's said in the next workspace", |ui| ui.composer_text() == "In hub.");
}

#[gpui_kit::test]
fn the_hub_restarting_leaves_it_dictating_as_it_leaves_whats_typed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.click("dictate");
    say(&mut ui, &engine, "fix the");
    hearing(&mut ui, "fix");

    let pid = stack.client().hub_status().unwrap().pid;
    client_core::stop_hub(&stack.hub.paths.socket).unwrap();
    ui.wait_for_new_hub(&stack, pid);

    assert!(live(&mut ui).is_some_and(|text| text.starts_with("fix")), "still dictating");
    say(&mut ui, &engine, "test");
    ui.click(dictation_ui::STOP);
    ui.wait("all of it, as one sentence", |ui| ui.composer_text() == "Fix the test.");
}

#[gpui_kit::test]
fn losing_the_hub_ends_it_since_the_composer_goes(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    // So the app can't start another hub once this one has gone.
    let daemon = stack.without_a_daemon();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.new_workspace();
    let engine = ready(&mut ui);
    ui.click("dictate");
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");
    let pane = ui.pane().unwrap();

    client_core::stop_hub(&stack.hub.paths.socket).unwrap();
    ui.wait("the app to find no hub", |ui| matches!(ui.view().linking(), Linking::Failed(_)));
    // Ended as the hub went, not as it comes back, so the microphone isn't on for as long as there's no hub.
    assert!(!ui.cx.read_entity(&pane, desktop::workspace_pane::WorkspacePane::dictating));
    local_hub::connect_or_start(&stack.hub.paths, &daemon, "test").unwrap();
    ui.click("try-hub-again");
    ui.wait("the hub to come back", |ui| matches!(ui.view().linking(), Linking::Ready));

    assert_eq!(live(&mut ui), None, "the composer comes back with nothing being dictated");
    assert_eq!(ui.composer_text(), "");
}

#[gpui_kit::test]
fn another_app_in_front_stops_dictating_as_letting_go_does_and_what_was_said_goes_in(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.bring_to_front();
    ui.new_workspace();
    let engine = ready(&mut ui);
    hold_option_space(&mut ui);
    say(&mut ui, &engine, "fix the test");
    hearing(&mut ui, "fix");

    // The key is let go of as the other app comes in front, which this one never hears.
    ui.send_to_back();
    ui.wait("the final text", |ui| ui.composer_text() == "Fix the test.");
    assert!(ui.has("dictate"), "the microphone stays, so what's beside it doesn't move");
    ui.bring_to_front();
    press_space(&mut ui);
    assert_eq!(ui.composer_text(), "Fix the test. ", "Space types again once the app is back");

    // Started from the button, it stops too, so the microphone isn't left on with nobody looking, even with Space down
    // for the other app, as ⌘Space would be.
    ui.click("dictate");
    say(&mut ui, &engine, "in hub");
    hearing(&mut ui, "in");
    on_the_keyboard(&mut ui, true);
    ui.send_to_back();
    ui.wait("the final text", |ui| ui.composer_text() == "Fix the test. In hub.");
    ended(&mut ui);
}
