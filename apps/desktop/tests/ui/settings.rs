//! Settings, which shows in place of the sidebar and the workspace, and what each setting changes.

#![cfg(test)]

use std::path::PathBuf;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, OwnedMenuItem, TestAppContext};

use crate::support::{Stack, Ui, labelled};

/// Whether Settings is showing, by its way back.
fn settings_showing(ui: &mut Ui<'_>) -> bool {
    ui.has("settings-back")
}

/// Opens Settings from the app menu, as the menu bar does.
fn open_settings(ui: &mut Ui<'_>) {
    let menus = ui.cx.update(|cx| cx.get_menus()).unwrap();
    let action = menus[0].items.iter().find_map(|item| match item {
        OwnedMenuItem::Action { name, action, .. } if name == "Settings…" => Some(action.boxed_clone()),
        _ => None,
    });
    ui.cx.update(|cx| cx.dispatch_action(action.unwrap().as_ref()));
    ui.wait("Settings", settings_showing);
}

/// Flips the switch for "Always expand thinking".
fn flip_always_expand_thinking(ui: &mut Ui<'_>) {
    ui.click_label("Always expand thinking");
}

/// Whether each recorded thinking block in the chat is open, top to bottom, as its line tells assistive apps.
fn thinking_open(ui: &mut Ui<'_>) -> Vec<bool> {
    ui.labelled("Thinking").iter().map(|line| line.expanded().unwrap()).collect()
}

/// The thinking shown in full, recorded or streaming, by element.
fn thinking_shown(ui: &mut Ui<'_>) -> Vec<ElementId> {
    ui.shown("thinking/")
}

/// Opens or closes the thinking line at this position, top to bottom.
fn click_thinking(ui: &mut Ui<'_>, position: usize) {
    ui.click_labelled("Thinking", position);
}

fn settings_file(stack: &Stack) -> PathBuf {
    stack.hub.paths.data_dir.join("settings.json")
}

/// Opens a workspace, with something typed in its composer.
fn typing<'a>(cx: &'a mut TestAppContext, stack: &Stack, typed: &str) -> Ui<'a> {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input(typed);
    ui
}

/// Opens a workspace and has its agent think, then answer.
fn after_thinking<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let mut ui = typing(cx, stack, "think:2:pondering; say:first answer");
    ui.press("enter");
    ui.wait_idle_with("the answer", |text| text.contains("first answer"));
    ui
}

#[gpui_kit::test]
fn settings_shows_in_place_of_the_workspace_and_goes_back_to_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing(cx, &stack, "half");
    assert!(ui.has("toggle-diff") && ui.has("add-project"));

    open_settings(&mut ui);
    assert!(!ui.has("toggle-diff"), "the workspace is hidden");
    assert!(!ui.has("add-project"), "and so is the sidebar");
    ui.click("settings-back");

    assert!(!settings_showing(&mut ui));
    assert!(ui.has("toggle-diff"));
    ui.input(" typed");
    assert_eq!(ui.composer_text(), "half typed", "focus is back where it was");
}

#[gpui_kit::test]
fn cmd_comma_opens_settings_and_escape_goes_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing(cx, &stack, "half");

    ui.press("cmd-,");
    assert!(settings_showing(&mut ui));
    ui.press("cmd-,");
    assert!(settings_showing(&mut ui), "still showing");
    ui.press("escape");

    assert!(!settings_showing(&mut ui));
    assert_eq!(ui.composer_text(), "half");
}

#[gpui_kit::test]
fn cmd_w_goes_back_from_settings_and_leaves_the_chat_open(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing(cx, &stack, "");
    open_settings(&mut ui);

    ui.press("cmd-w");

    assert!(!settings_showing(&mut ui));
    assert_eq!(ui.open_chats().len(), 1);
}

#[gpui_kit::test]
fn the_workspaces_keys_do_nothing_while_settings_shows(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing(cx, &stack, "");
    open_settings(&mut ui);

    ui.press("cmd-shift-d");
    ui.press("escape");

    assert!(ui.changes_open(), "the sidebar of changes is still open");
}

#[gpui_kit::test]
fn always_expand_thinking_opens_thinking_and_is_saved(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = after_thinking(cx, &stack);
    assert_eq!(thinking_open(&mut ui), [false], "thinking starts as a line to open");
    assert!(thinking_shown(&mut ui).is_empty());

    open_settings(&mut ui);
    flip_always_expand_thinking(&mut ui);
    ui.click("settings-back");

    assert_eq!(thinking_open(&mut ui), [true]);
    assert_eq!(thinking_shown(&mut ui).len(), 1);
    ui.wait("the setting to be saved", |_| {
        std::fs::read_to_string(settings_file(&stack))
            .is_ok_and(|saved| saved.contains(r#""always_expand_thinking": true"#))
    });
    open_settings(&mut ui);
    flip_always_expand_thinking(&mut ui);
    ui.click("settings-back");
    assert_eq!(thinking_open(&mut ui), [false], "turned off again");
}

#[gpui_kit::test]
fn a_setting_changed_just_before_quitting_is_saved(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing(cx, &stack, "");
    open_settings(&mut ui);
    let id = ui.cx.update_window(ui.handle, |_, window, _| labelled(window, "Always expand thinking")).unwrap();

    // Straight after, with nothing let run in between.
    ui.cx.update_window(ui.handle, |_, window, cx| window.click(id, cx)).unwrap();
    ui.cx.update(gpui_kit::App::shutdown);

    let saved = std::fs::read_to_string(settings_file(&stack)).unwrap();
    assert!(saved.contains(r#""always_expand_thinking": true"#), "{saved}");
}

#[gpui_kit::test]
fn thinking_closed_by_hand_stays_closed_while_new_thinking_opens(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = after_thinking(cx, &stack);
    open_settings(&mut ui);
    flip_always_expand_thinking(&mut ui);
    ui.click("settings-back");
    assert_eq!(thinking_open(&mut ui), [true]);

    click_thinking(&mut ui, 0);
    assert_eq!(thinking_open(&mut ui), [false]);
    stack.client().session_prompt(&ui.active_chat(), "think:2:again; say:second answer", None).unwrap();
    ui.wait_idle_with("the second answer", |text| text.contains("second answer"));

    assert_eq!(thinking_open(&mut ui), [false, true], "only the new thinking is open");
}

#[gpui_kit::test]
fn a_saved_setting_is_in_force_from_the_start_and_thinking_streams_in_full(cx: &mut TestAppContext) {
    let stack = Stack::new();
    std::fs::create_dir_all(&stack.hub.paths.data_dir).unwrap();
    std::fs::write(settings_file(&stack), r#"{"always_expand_thinking": true}"#).unwrap();
    let mut ui = typing(cx, &stack, "think:30:idea");

    ui.press("enter");
    ui.wait("thinking to stream", |ui| {
        ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| t.streaming_text().contains("idea3")))
    });

    let streaming = thinking_shown(&mut ui);
    assert_eq!(streaming.len(), 1);
    ui.wait("the thinking to finish", |ui| ui.session_status() == Some(protocol::model::SessionStatus::Idle));
    assert_eq!(thinking_open(&mut ui), [true]);
    assert_eq!(thinking_shown(&mut ui), streaming, "the same text, recorded");
}
