//! The new-workspace dialog: what it makes, what the first prompt carries, and what's kept when it closes.

#![cfg(test)]

use gpui_kit::{AppContext as _, ClipboardEntry, ClipboardItem, ExternalPaths, SharedString, TestAppContext};
use protocol::event::EventBody;
use protocol::model::{DEFAULT_EFFORT, DEFAULT_MODEL, Effort, ModelName, PermissionMode};

use crate::support::{Stack, Ui, git};

mod given_back;

fn open_with_a_repo<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui
}

/// The text of each prompt the chat showing has been sent, in order.
fn prompts(ui: &Ui<'_>) -> Vec<String> {
    let prompts = ui.events().into_iter().filter_map(|event| match event.body {
        EventBody::UserPrompt { text, .. } => Some(text),
        _ => None,
    });
    prompts.collect()
}

#[gpui_kit::test]
fn what_was_typed_is_the_new_workspaces_first_prompt(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    let before = ui.view().workspaces.len();

    ui.open_new_workspace();
    ui.input("say:on it");
    ui.press("enter");

    let workspace = ui.wait_for_a_new_workspace(before);
    assert!(!ui.has_dialog(), "Create closes the dialog");
    ui.wait_idle_with("the agent to answer the first prompt", |text| text == "on it");
    assert_eq!(prompts(&ui), ["say:on it"]);
    let made = ui.view().workspaces.iter().find(|w| w.id == workspace).map(|w| w.repo_id.clone());
    assert_eq!(made, Some(ui.first_repo_id()));
    assert_eq!(ui.dialog_text(), "", "the next workspace starts with an empty dialog");
}

#[gpui_kit::test]
fn creating_with_nothing_typed_sends_nothing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);

    ui.new_workspace();

    assert!(prompts(&ui).is_empty());
    assert!(!ui.events().iter().any(|e| matches!(e.body, EventBody::AgentStarted { .. })), "no agent was started");
}

#[gpui_kit::test]
fn the_model_and_effort_picked_in_the_dialog_are_the_first_chats(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    let before = ui.view().workspaces.len();

    ui.open_new_workspace();
    let at_first = ui.cx.read_entity(&ui.dialog(), |d, _| d.chosen());
    assert_eq!(at_first, (DEFAULT_MODEL, DEFAULT_EFFORT), "what the hub gives a chat that names neither");
    ui.pick_item("new-workspace-model", "Sonnet");
    ui.pick_item("new-workspace-model", "High");
    ui.input("effort");
    ui.click("new-workspace-create");

    ui.wait_for_a_new_workspace(before);
    ui.wait_idle_with("the agent to say its effort", |text| text == "effort=high");
    let chat = stack.client().session_get(&ui.active_chat()).unwrap();
    assert_eq!((chat.model, chat.effort), (ModelName::named("sonnet"), Effort::High));
    let launched = ui.events().into_iter().find_map(|e| match e.body {
        EventBody::AgentInit { model, .. } => model,
        _ => None,
    });
    assert_eq!(launched.as_deref(), Some("sonnet"), "the agent ran with it from its first prompt");
    let kept = ui.cx.read_entity(&ui.dialog(), |d, _| d.chosen());
    assert_eq!(kept, (ModelName::named("sonnet"), Effort::High), "the next workspace starts with them too");
}

#[gpui_kit::test]
fn closing_the_dialog_keeps_what_was_typed_for_next_time(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);

    ui.press("cmd-n");
    ui.wait("the dialog", Ui::has_dialog);
    ui.input("half a thought");
    ui.press("escape");

    assert!(!ui.has_dialog());
    assert!(ui.view().workspaces.is_empty(), "closing makes nothing");
    ui.open_new_workspace();
    assert_eq!(ui.dialog_text(), "half a thought");
}

#[gpui_kit::test]
fn the_dialog_can_make_the_workspace_in_another_repository(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let other = stack.dir.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    git(&other, &["init", "-q", "-b", "main"]);
    git(&other, &["commit", "-q", "--allow-empty", "-m", "first"]);
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    client.repo_add(other.to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("both repositories", |ui| ui.view().repos.len() == 2);
    // The menu lists them as the sidebar does, and the dialog opens for the first.
    let second = ui.view().repos[1].id.clone();

    ui.open_new_workspace();
    ui.pick("new-workspace-repo", 1);
    let workspace = ui.create_workspace();

    let made = ui.view().workspaces.iter().find(|w| w.id == workspace).map(|w| w.repo_id.clone());
    assert_eq!(made, Some(second));
}

#[gpui_kit::test]
fn a_file_pasted_in_the_dialog_goes_with_the_first_prompt(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    let notes = stack.dir.path().join("notes.txt");
    std::fs::write(&notes, "the first line\n").unwrap();
    let before = ui.view().workspaces.len();

    ui.open_new_workspace();
    ui.paste(ClipboardItem::from(ClipboardEntry::ExternalPaths(ExternalPaths(vec![notes].into()))));
    ui.wait("the file to be attached", |ui| ui.dialog_attached() == ["notes.txt"]);
    assert_eq!(ui.dialog_text(), "", "a copied file isn't text");
    ui.input("attached");
    ui.press("enter");

    ui.wait_for_a_new_workspace(before);
    ui.wait_idle_with("the agent to see the file", |text| text.contains("notes.txt"));
    let carried = ui.events().into_iter().find_map(|e| match e.body {
        EventBody::UserPrompt { attachments, .. } => Some(attachments),
        _ => None,
    });
    let names: Vec<String> = carried.unwrap_or_default().into_iter().map(|a| a.name).collect();
    assert_eq!(names, ["notes.txt"]);
    assert!(ui.dialog_attached().is_empty(), "creating takes what was attached");
}

/// Turned on with `Shift-Tab` or the `+` menu, as in the composer, it's the first chat's, whose agent takes the first
/// prompt in it, and the next workspace starts without it.
#[gpui_kit::test]
fn plan_mode_turned_on_in_the_dialog_is_the_first_chats(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    let before = ui.view().workspaces.len();

    ui.open_new_workspace();
    ui.press("shift-tab");
    assert!(ui.dialog_planning(), "Shift-Tab turns it on");
    assert!(ui.shows_label("new-workspace", "Plan mode"), "the dialog shows it");
    assert_eq!(ui.dialog_text(), "", "Shift-Tab types nothing");
    ui.click(SharedString::from("new-workspace-add"));
    ui.click_menu_item("Plan mode");
    assert!(!ui.dialog_planning(), "the menu turns it off");
    ui.click(SharedString::from("new-workspace-add"));
    ui.click_menu_item("Plan mode");
    assert!(ui.dialog_planning(), "and on again");
    ui.input("say:a plan");
    ui.press("enter");

    ui.wait_for_a_new_workspace(before);
    ui.wait_idle_with("the agent to answer the first prompt", |text| text == "a plan");
    assert_eq!(ui.chat_mode(), Some(PermissionMode::Plan));
    let took_it_in = ui.events().into_iter().find_map(|e| match e.body {
        EventBody::AgentInit { permission_mode, .. } => permission_mode,
        _ => None,
    });
    assert_eq!(took_it_in, Some(PermissionMode::Plan), "the agent took the first prompt in plan mode");
    ui.open_new_workspace();
    assert!(!ui.dialog_planning(), "the next workspace starts with full access");
    assert!(!ui.shows_label("new-workspace", "Plan mode"));
}

#[gpui_kit::test]
fn files_chosen_from_the_dialogs_menu_are_attached(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    let notes = stack.dir.path().join("notes.txt");
    std::fs::write(&notes, "the first line\n").unwrap();

    ui.open_new_workspace();
    ui.click(SharedString::from("new-workspace-add"));
    ui.click_menu_item("Attach files…");
    ui.wait("the file picker", |ui| ui.cx.did_prompt_for_paths());
    ui.cx.simulate_path_prompt_response(|_| Some(vec![notes.clone()]));
    ui.wait("the file to be attached", |ui| ui.dialog_attached() == ["notes.txt"]);
}
