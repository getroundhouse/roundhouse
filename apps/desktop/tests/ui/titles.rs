//! What workspaces and chats are called, renaming them from their menus, and asking for a workspace's title again.

#![cfg(test)]

use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, SharedString, TestAppContext};

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
    assert!(ui.shows_label(&tab, "Untitled"), "until it has a title");

    // It starts from nothing, so Return alone can't make Untitled the chat's title.
    rename(&mut ui, &tab);
    ui.press("enter");
    assert!(ui.has_dialog(), "an empty title keeps the dialog open");
    ui.input("First try");
    ui.click(SharedString::from("rename-confirm"));

    ui.wait("the chat's new title", |ui| ui.shows_label(&tab, "First try"));
    assert!(!ui.has_dialog());
    assert_eq!(client.session_get(&chat).unwrap().title.as_deref(), Some("First try"));

    // Once it has a title, renaming starts from that.
    rename(&mut ui, &tab);
    ui.press("enter");
    assert!(!ui.has_dialog(), "Return alone keeps the title it has");
    let started = ui.view().client().unwrap().commands();
    started.finished().recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(client.session_get(&chat).unwrap().title.as_deref(), Some("First try"));
}

/// The fake agent titles only a prompt that carries its title, after `titled:`, and fails any other, as the CLI does
/// when it isn't logged in.
#[gpui_kit::test]
fn a_new_title_is_asked_for_from_the_header_and_the_rows_menu(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    client.repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let workspace = ui.new_workspace();
    let row = format!("workspace-{workspace}");
    assert!(ui.disabled(SharedString::from("regenerate-title")), "nothing has been asked, so there's none to make");
    ui.input("say:fix the login");
    ui.press("enter");
    ui.wait_idle_with("the answer", |text| text.contains("fix the login"));
    ui.wait("the title", |ui| ui.shows_label(&row, "say:fix the login"));

    // With no title to be had, it says so and keeps the one it has.
    ui.click(SharedString::from("regenerate-title"));
    ui.wait("why there's no new title", |ui| {
        ui.view().error.as_deref().is_some_and(|error| error.starts_with("couldn't regenerate the title: "))
    });
    assert!(ui.shows_label(&row, "say:fix the login"));
    assert!(!ui.disabled(SharedString::from("regenerate-title")), "and can be asked again");

    ui.input("say:and test it titled:Fix and test the login");
    ui.press("enter");
    ui.wait_idle_with("the second answer", |text| text.contains("and test it"));
    ui.right_click(SharedString::from(row.clone()));
    ui.click_label("Regenerate title");
    ui.wait("the new title", |ui| ui.shows_label(&row, "Fix and test the login"));
    assert!(ui.shows_label("workspace-title", "Fix and test the login"), "the header too");

    // Over a title typed by hand too, since it's asked for.
    rename(&mut ui, &row);
    ui.input("Login bug");
    ui.press("enter");
    ui.wait("the typed title", |ui| ui.shows_label(&row, "Login bug"));
    ui.click(SharedString::from("regenerate-title"));
    ui.wait("the title again", |ui| ui.shows_label(&row, "Fix and test the login"));
    let stored = client.workspace_list(false).unwrap().into_iter().find(|w| w.id == workspace).unwrap();
    assert_eq!(stored.title.as_deref(), Some("Fix and test the login"));

    // A title as long as one can be gives way to the button, rather than pushing it out of the header.
    let long = "word ".repeat(24);
    rename(&mut ui, &row);
    ui.input(&long);
    ui.press("enter");
    ui.wait("the long title", |ui| ui.shows_label("workspace-title", long.trim()));
    let mut bounds =
        |id: &'static str| ui.cx.update_window(ui.handle, |_, window, _| window.find(id).bounds()).unwrap();
    let (button, header_end) = (bounds("regenerate-title"), bounds("toggle-diff"));
    assert!(button.right() <= header_end.left(), "{button:?} is past the header's end, {header_end:?}");
}
