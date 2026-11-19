#![cfg(test)]

use crate::menus::{SETTINGS, choose};
use crate::support;
use crate::support::{FakeHub, Gate, Stack, Ui, labelled};
use client_core::fake_hub::World;
use gpui_kit::{AppContext as _, ElementId, TestAppContext};

use super::made_up::in_a_workspace;

/// Whether Settings is showing, by its way back.
pub(super) fn settings_showing(ui: &mut Ui<'_>) -> bool {
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
    let mut ui = typing(cx, stack, "ponder:1:1; say:first answer");
    ui.press("enter");
    ui.wait_idle_with("the answer", |text| text.contains("first answer"));
    ui
}

/// Whether the app's settings file says thinking is always expanded, as `on` says.
fn saved_as(fake: &FakeHub, on: bool) -> bool {
    std::fs::read_to_string(fake.settings_file())
        .is_ok_and(|saved| saved.contains(&format!(r#""always_expand_thinking": {on}"#)))
}

/// Every way into Settings and back, in one window on a fake hub, since none asks the hub for anything: from the menu
/// and back by its button, by Cmd-, and back by Escape, and back by Cmd-W, with the workspace's keys doing nothing
/// meanwhile.
#[gpui_kit::test]
fn settings_shows_in_place_of_the_workspace_and_each_way_back_leaves_it_as_it_was(cx: &mut TestAppContext) {
    let (_hub, mut ui) = in_a_workspace(cx);
    ui.input("half");
    assert!(ui.has("toggle-diff") && ui.has("add-project"));
    assert!(ui.changes_open(), "the sidebar of changes starts open");

    open_settings(&mut ui);
    assert!(!ui.has("toggle-diff"), "the workspace is hidden");
    assert!(!ui.has("add-project"), "and so is the sidebar");
    ui.click("settings-back");
    assert!(!settings_showing(&mut ui), "the button goes back");
    assert!(ui.has("toggle-diff"));
    ui.input(" typed");
    assert_eq!(ui.composer_text(), "half typed", "focus is back where it was, after the button");

    ui.press("secondary-,");
    assert!(settings_showing(&mut ui), "Cmd-, opens it");
    ui.press("secondary-,");
    assert!(settings_showing(&mut ui), "Cmd-, again leaves it showing");
    ui.press("secondary-shift-b");
    ui.press("escape");
    assert!(!settings_showing(&mut ui), "Escape goes back");
    assert!(ui.changes_open(), "the sidebar of changes is still open, since its key did nothing in Settings");
    assert_eq!(ui.composer_text(), "half typed", "and Escape left the composer as it was");

    open_settings(&mut ui);
    ui.press("secondary-w");
    assert!(!settings_showing(&mut ui), "Cmd-W goes back");
    // A fake hub refuses to close a chat, so the chat staying open proves nothing alone. The window would say it
    // couldn't close it.
    ui.wait_for_every_call();
    assert_eq!(ui.open_chats().len(), 1, "Cmd-W leaves the chat open");
    assert_eq!(ui.view().error, None, "and asked the hub to close nothing");
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
    ui.wait("the other one, behind Settings", |ui| ui.view().selected() == Some(&other) && ui.pane().is_some());
    ui.press("escape");

    assert!(!settings_showing(&mut ui), "the keys still reach Settings");
    ui.input("here");
    assert_eq!(ui.composer_text(), "here", "and then the workspace showing");
}

/// The setting is the app's own, kept in its settings file, so a fake hub will do.
#[gpui_kit::test]
fn always_expand_thinking_opens_thinking_and_is_saved(cx: &mut TestAppContext) {
    let (hub, mut ui) = in_a_workspace(cx);
    assert_eq!(thinking_open(&mut ui), [false], "thinking starts as a line to open");
    assert!(thinking_shown(&mut ui).is_empty());

    open_settings(&mut ui);
    flip_always_expand_thinking(&mut ui);
    ui.click("settings-back");

    assert_eq!(thinking_open(&mut ui), [true]);
    assert_eq!(thinking_shown(&mut ui).len(), 1);
    ui.wait("the setting to be saved", |_| saved_as(&hub, true));
    open_settings(&mut ui);
    flip_always_expand_thinking(&mut ui);
    ui.click("settings-back");
    assert_eq!(thinking_open(&mut ui), [false], "turned off again");
}

#[gpui_kit::test]
fn a_setting_changed_just_before_quitting_is_saved(cx: &mut TestAppContext) {
    let (hub, mut ui) = in_a_workspace(cx);
    open_settings(&mut ui);
    let id = ui.cx.update_window(ui.handle, |_, window, _| labelled(window, "Always expand thinking")).unwrap();

    // Straight after, with nothing let run in between.
    ui.cx.update_window(ui.handle, |_, window, cx| support::click(window, id, cx)).unwrap();
    ui.cx.update(gpui_kit::App::shutdown);

    let saved = std::fs::read_to_string(hub.settings_file()).unwrap();
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
    stack.client().session_prompt(&ui.active_chat(), "ponder:1:2; say:second answer", None).unwrap();
    ui.wait_idle_with("the second answer", |text| text.contains("second answer"));

    assert_eq!(thinking_open(&mut ui), [false, true], "only the new thinking is open");
}

#[gpui_kit::test]
fn a_saved_setting_is_in_force_from_the_start_and_thinking_streams_in_full(cx: &mut TestAppContext) {
    let stack = Stack::new();
    std::fs::create_dir_all(&stack.hub.paths.data_dir).unwrap();
    std::fs::write(stack.settings_file(), r#"{"always_expand_thinking": true}"#).unwrap();
    // Thinking that goes on until the test has seen it shown while it streams.
    let thought = Gate::new(&stack, "thought");
    let mut ui = typing(cx, &stack, "");
    ui.paste_text(&format!("think-until:{thought}:idea"));

    ui.press("enter");
    ui.wait("thinking to stream", |ui| {
        ui.transcript().is_some_and(|t| ui.cx.read_entity(&t, |t, _| t.streaming_text().contains("idea3")))
    });

    let streaming = thinking_shown(&mut ui);
    assert_eq!(streaming.len(), 1);
    thought.open();
    // The chat's record says it's idle on the hub's feed, apart from its events, so the recorded thinking is waited
    // for, with the end of its turn, rather than read as soon as the chat is idle.
    ui.wait("the thinking to finish, and its turn to be drawn to its end", |ui| {
        ui.session_status() == Some(protocol::model::SessionStatus::Idle) && ui.turn_drawn_to_its_end()
    });
    assert_eq!(thinking_open(&mut ui), [true]);
    assert_eq!(thinking_shown(&mut ui), streaming, "the same text, recorded");
}

/// What the switch for the hub keeping the computer awake is called.
pub(super) const KEEP_AWAKE: &str = "Keep computer awake while agents are working";

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

/// Each of the hub's own settings that General shows, as one walk through one window, since each switch is the same
/// control: what was chosen before the app opened, each switch flipped both ways and the other left as it was, and
/// what another client chose. What a hub starts out with is the hub's own rule, which its store's tests pin.
#[gpui_kit::test]
fn each_switch_for_the_hubs_own_settings_changes_it_and_shows_what_the_hub_says_whoever_chose_it(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let change = protocol::model::HubSettingsChange { keep_awake: Some(false), ..Default::default() };
    stack.client().hub_configure(change).unwrap();
    let mut ui = Ui::open(cx, &stack);
    open_settings(&mut ui);
    ui.wait("Keep awake to show what was chosen before the app opened", |ui| keep_awake_switch(ui) == (false, true));
    ui.wait("Skip safety checks to show the hub asks the person", |ui| switch(ui, SKIP_SAFETY_CHECKS) == (false, true));

    ui.click_label(KEEP_AWAKE);
    ui.wait("the hub to be told to keep the computer awake", |_| hub_keeps_awake(&stack));
    ui.wait("Keep awake to show on", |ui| keep_awake_switch(ui) == (true, true));
    assert_eq!(switch(&mut ui, SKIP_SAFETY_CHECKS), (false, true), "Skip safety checks is left as it was");
    let saved = std::fs::read_to_string(stack.settings_file()).unwrap_or_default();
    assert!(!saved.contains("keep_awake"), "it's the hub's to keep, not the app's: {saved}");
    ui.click_label(KEEP_AWAKE);
    ui.wait("the hub to be told to let the computer sleep again", |_| !hub_keeps_awake(&stack));
    ui.wait("Keep awake to show off again", |ui| keep_awake_switch(ui) == (false, true));

    ui.click_label(SKIP_SAFETY_CHECKS);
    ui.wait("the hub to be told to skip the safety checks", |_| hub_allows_what_full_access_asks(&stack) == Some(true));
    ui.wait("Skip safety checks to show on", |ui| switch(ui, SKIP_SAFETY_CHECKS) == (true, true));
    assert_eq!(keep_awake_switch(&mut ui), (false, true), "Keep awake is left as it was");
    ui.click_label(SKIP_SAFETY_CHECKS);
    ui.wait("the hub to be told to ask again", |_| hub_allows_what_full_access_asks(&stack) == Some(false));
    ui.wait("Skip safety checks to show off again", |ui| switch(ui, SKIP_SAFETY_CHECKS) == (false, true));

    let change = protocol::model::HubSettingsChange { keep_awake: Some(true), ..Default::default() };
    stack.client().hub_configure(change).unwrap();

    ui.wait("Keep awake to show what another client chose", |ui| keep_awake_switch(ui) == (true, true));
}

/// A hub from before hubs kept settings sends none, which a fake hub can be.
#[gpui_kit::test]
fn the_switch_cant_be_flipped_with_a_hub_from_before_it_kept_settings(cx: &mut TestAppContext) {
    let mut older = World::default();
    older.keeps_no_settings = true;
    let fake = FakeHub::holding(older);
    let mut ui = Ui::open_on(cx, &fake);
    open_settings(&mut ui);

    assert_eq!(keep_awake_switch(&mut ui), (false, false), "an older hub doesn't keep the computer awake at all");
    assert_eq!(switch(&mut ui, SKIP_SAFETY_CHECKS), (false, false), "nor allow a tool's use itself");
}
