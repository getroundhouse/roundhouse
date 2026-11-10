//! The new-workspace dialog: what it makes, what the first prompt carries, and what's kept when it closes.

#![cfg(test)]

use gpui_kit::{AppContext as _, ClipboardEntry, ClipboardItem, ExternalPaths, TestAppContext};
use protocol::event::EventBody;
use protocol::model::Effort;

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
    ui.pick_item("new-workspace-model", "Sonnet");
    ui.pick_item("new-workspace-model", "Extra high");
    ui.input("effort");
    ui.click("new-workspace-create");

    ui.wait_for_a_new_workspace(before);
    ui.wait_idle_with("the agent to say its effort", |text| text == "effort=xhigh");
    let chat = stack.client().session_get(&ui.active_chat()).unwrap();
    assert_eq!((chat.model.as_deref(), chat.effort), (Some("sonnet"), Some(Effort::Xhigh)));
    let launched = ui.events().into_iter().find_map(|e| match e.body {
        EventBody::AgentInit { model, .. } => model,
        _ => None,
    });
    assert_eq!(launched.as_deref(), Some("sonnet"), "the agent ran with it from its first prompt");
    let kept = ui.cx.read_entity(&ui.dialog(), |d, _| d.chosen());
    assert_eq!(kept, (Some("sonnet".to_string()), Some(Effort::Xhigh)), "the next workspace starts with them too");
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
