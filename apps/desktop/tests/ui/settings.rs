//! Settings, which shows in place of the sidebar and the workspace, and what each setting changes.

#![cfg(test)]

use gpui_kit::{AppContext as _, ElementId, TestAppContext};

use crate::menus::{SETTINGS, choose};
use crate::support::{self, Stack, Ui, labelled};

mod accounts;
mod search;

/// Whether Settings is showing, by its way back.
fn settings_showing(ui: &mut Ui<'_>) -> bool {
    ui.has("settings-back")
}

/// Opens Settings from the menus, as the menu bar does: the app menu on a Mac, and File elsewhere.
pub(crate) fn open_settings(ui: &mut Ui<'_>) {
    choose(ui, SETTINGS.0, SETTINGS.1);
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

/// Opens a workspace, with something typed in its composer.
fn typing<'a>(cx: &'a mut TestAppContext, stack: &Stack, typed: &str) -> Ui<'a> {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
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

    ui.press("secondary-,");
    assert!(settings_showing(&mut ui));
    ui.press("secondary-,");
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

    ui.press("secondary-w");

    assert!(!settings_showing(&mut ui));
    assert_eq!(ui.open_chats().len(), 1);
}

/// Archived from elsewhere while Settings shows, so the app selects another workspace behind it, whose composer isn't
/// drawn until Settings goes.
#[gpui_kit::test]
fn a_workspace_selected_behind_settings_has_the_keys_once_settings_goes(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let other = ui.new_workspace();
    let showing = ui.new_workspace();
    open_settings(&mut ui);

    stack.client().workspace_archive(&showing, false).unwrap();
    ui.wait("the other one, behind Settings", |ui| ui.view().selected.as_ref() == Some(&other) && ui.pane().is_some());
    ui.press("escape");

    assert!(!settings_showing(&mut ui), "the keys still reach Settings");
    ui.input("here");
    assert_eq!(ui.composer_text(), "here", "and then the workspace showing");
}

#[gpui_kit::test]
fn the_workspaces_keys_do_nothing_while_settings_shows(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing(cx, &stack, "");
    open_settings(&mut ui);

    ui.press("secondary-shift-d");
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
        std::fs::read_to_string(stack.settings_file())
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
    ui.cx.update_window(ui.handle, |_, window, cx| support::click(window, id, cx)).unwrap();
    ui.cx.update(gpui_kit::App::shutdown);

    let saved = std::fs::read_to_string(stack.settings_file()).unwrap();
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
    std::fs::write(stack.settings_file(), r#"{"always_expand_thinking": true}"#).unwrap();
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

/// What the switch for the hub keeping the computer awake is called.
const KEEP_AWAKE: &str = "Keep computer awake while agents are working";

/// What the hub's own settings say about keeping the computer awake, asked by a second client.
fn hub_keeps_awake(stack: &Stack) -> bool {
    stack.client().records_list().unwrap().hub_settings.unwrap().keep_awake
}

/// What the switch for the hub allowing what Claude's safety checks ask about is called.
const SKIP_SAFETY_CHECKS: &str = "Skip Claude's safety checks";

/// Whether the switch called `name` shows on, and whether it can be flipped.
fn switch(ui: &mut Ui<'_>, name: &str) -> (bool, bool) {
    let switch = &ui.labelled(name)[0];
    (switch.checked() == Some(true), switch.disabled() != Some(true))
}

/// Whether the switch for keeping the computer awake shows on, and whether it can be flipped.
fn keep_awake_switch(ui: &mut Ui<'_>) -> (bool, bool) {
    switch(ui, KEEP_AWAKE)
}

/// What the hub's own settings say about allowing a tool's use itself, asked by a second client.
fn hub_allows_what_full_access_asks(stack: &Stack) -> Option<bool> {
    stack.client().records_list().unwrap().hub_settings.unwrap().allow_asked_in_full_access
}

#[gpui_kit::test]
fn the_switch_for_skipping_claudes_safety_checks_changes_the_hubs_own_setting(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing(cx, &stack, "");
    open_settings(&mut ui);
    ui.wait("the switch to show the hub asks the person", |ui| switch(ui, SKIP_SAFETY_CHECKS) == (false, true));

    ui.click_label(SKIP_SAFETY_CHECKS);

    ui.wait("the hub to be told", |_| hub_allows_what_full_access_asks(&stack) == Some(true));
    ui.wait("the switch to show on", |ui| switch(ui, SKIP_SAFETY_CHECKS) == (true, true));
    assert_eq!(keep_awake_switch(&mut ui), (true, true), "the other is left as it was");
    ui.click_label(SKIP_SAFETY_CHECKS);
    ui.wait("the hub to be told again", |_| hub_allows_what_full_access_asks(&stack) == Some(false));
}

#[gpui_kit::test]
fn the_switch_for_keeping_the_computer_awake_changes_the_hubs_own_setting(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing(cx, &stack, "");
    open_settings(&mut ui);
    ui.wait("the switch to show what a hub starts out doing", |ui| keep_awake_switch(ui) == (true, true));

    ui.click_label(KEEP_AWAKE);

    ui.wait("the hub to be told", |_| !hub_keeps_awake(&stack));
    ui.wait("the switch to show off", |ui| keep_awake_switch(ui) == (false, true));
    let saved = std::fs::read_to_string(stack.settings_file()).unwrap_or_default();
    assert!(!saved.contains("keep_awake"), "it's the hub's to keep, not the app's: {saved}");
    ui.click_label(KEEP_AWAKE);
    ui.wait("the hub to be told again", |_| hub_keeps_awake(&stack));
}

#[gpui_kit::test]
fn the_switch_shows_what_the_hub_says_whoever_chose_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let change = protocol::model::HubSettingsChange { keep_awake: Some(false), ..Default::default() };
    stack.client().hub_configure(change).unwrap();
    let mut ui = typing(cx, &stack, "");
    open_settings(&mut ui);
    ui.wait("the switch to show what was chosen before the app opened", |ui| keep_awake_switch(ui) == (false, true));

    let change = protocol::model::HubSettingsChange { keep_awake: Some(true), ..Default::default() };
    stack.client().hub_configure(change).unwrap();

    ui.wait("the switch to show what another client chose", |ui| keep_awake_switch(ui) == (true, true));
}

#[gpui_kit::test]
fn the_switch_cant_be_flipped_with_a_hub_from_before_it_kept_settings(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.hub.build_number = Some(2);
    let _older = stack.start_stand_in_hub(&["--speaks-this-protocol", "--build", "1", "--not-on-demand"]);
    let mut ui = Ui::open(cx, &stack);
    open_settings(&mut ui);

    assert_eq!(keep_awake_switch(&mut ui), (false, false), "an older hub doesn't keep the computer awake at all");
    assert_eq!(switch(&mut ui, SKIP_SAFETY_CHECKS), (false, false), "nor allow a tool's use itself");
}
