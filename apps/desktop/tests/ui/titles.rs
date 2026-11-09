//! What workspaces and chats are called, and renaming them from their menus.

#![cfg(test)]

use std::time::Duration;

use gpui_kit::{SharedString, TestAppContext};

use crate::support::{Stack, Ui};

/// Right-clicks something and picks Rename…, wherever it is in the menu.
fn rename(ui: &mut Ui, id: &str) {
    ui.right_click(SharedString::from(id.to_string()));
    ui.click_label("Rename…");
}

#[gpui_kit::test]
fn the_first_prompt_titles_the_workspace_and_a_rename_replaces_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let workspace = ui.new_workspace();
    let row = format!("workspace-{workspace}");
    let name = ui.view().workspaces.iter().find(|w| w.id == workspace).unwrap().name.clone();
    assert!(ui.shows_label(&row, &name), "the name, until there's a prompt");
    ui.input("say:fix the login");
    ui.press("enter");
    ui.wait_idle_with("the answer", |text| text.contains("fix the login"));
    ui.wait("the title", |ui| ui.shows_label(&row, "say:fix the login"));

    // The title is selected, so typing replaces it, and Enter renames.
    rename(&mut ui, &row);
    assert!(ui.has_dialog());
    ui.input("Login bug");
    ui.press("enter");
    ui.wait("the new title", |ui| ui.shows_label(&row, "Login bug"));
    assert!(!ui.has_dialog());
    assert!(ui.shows_label("workspace-title", "Login bug"), "the header too");

    // Cancelling changes nothing, even once every call the app started has finished.
    rename(&mut ui, &row);
    ui.input("Never mind");
    ui.press("escape");
    assert!(!ui.has_dialog());
    let started = ui.view().client().unwrap().commands();
    started.finished().recv_timeout(Duration::from_secs(10)).unwrap();
    let stored = client.workspace_list(false).unwrap().into_iter().find(|w| w.id == workspace).unwrap();
    assert_eq!((stored.title.as_deref(), stored.name), (Some("Login bug"), name));
}

#[gpui_kit::test]
fn a_chat_is_renamed_from_its_tab(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    let chat = ui.active_chat();
    let tab = format!("chat-{chat}");

    rename(&mut ui, &tab);
    ui.input("First try");
    ui.click(SharedString::from("rename-confirm"));

    ui.wait("the chat's new title", |ui| ui.shows_label(&tab, "First try"));
    assert!(!ui.has_dialog());
    assert_eq!(client.session_get(&chat).unwrap().title.as_deref(), Some("First try"));
}
