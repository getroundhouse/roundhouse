//! Home, which lists every workspace in place of the one showing.

#![cfg(test)]

use gpui_kit::{SharedString, TestAppContext};
use protocol::model::{WorkspaceId, WorkspaceStatus};

use crate::support::{Stack, Ui, home_row};

/// A window with a repository and two workspaces, the second showing, and Home opened over it. Returns them.
fn open_home<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, WorkspaceId, WorkspaceId) {
    let mut ui = Ui::open_with_a_repository(cx, stack);
    let first = ui.new_workspace();
    let second = ui.new_workspace();
    ui.click("home");
    ui.wait("Home", |ui| ui.has("home-page"));
    (ui, first, second)
}

#[gpui_kit::test]
fn home_shows_in_place_of_the_workspace_until_one_is_picked(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, first, second) = open_home(cx, &stack);

    assert!(!ui.has("workspace-title"), "the workspace under it is hidden");
    assert!(ui.has(home_row(&first)) && ui.has(home_row(&second)));
    ui.click(home_row(&first));

    ui.wait("the workspace", |ui| ui.view().selected.as_ref() == Some(&first) && ui.has("workspace-title"));
    assert!(!ui.has("home-page"));
}

#[gpui_kit::test]
fn the_list_narrows_to_what_the_search_matches_as_its_typed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, first, second) = open_home(cx, &stack);
    let branch = ui.view().workspaces.iter().find(|w| w.id == first).unwrap().branch.clone();

    // Home's search has the keys, since the composer under it is hidden.
    ui.input(&branch);

    ui.wait("the other workspace to go", |ui| !ui.has(home_row(&second)));
    assert!(ui.has(home_row(&first)));
}

#[gpui_kit::test]
fn the_workspaces_keys_do_nothing_while_home_hides_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, _, _) = open_home(cx, &stack);
    assert!(ui.changes_open(), "it starts open");

    ui.press("cmd-shift-d");

    assert!(ui.changes_open(), "the sidebar of the workspace nobody can see stays as it was");
}

#[gpui_kit::test]
fn archiving_from_home_leaves_home_showing_with_the_keys(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, _, second) = open_home(cx, &stack);
    let archived =
        |ui: &Ui| ui.view().workspaces.iter().any(|w| w.id == second && w.status == WorkspaceStatus::Archived);

    // The one selected, so the selection moves to another under Home once it's archived.
    ui.click(SharedString::from(format!("home-archive-{second}")));

    ui.wait("it to be archived", |ui| archived(ui) && !ui.has(home_row(&second)));
    assert!(ui.has("home-page"), "the button doesn't also show the workspace, as its row would");
    ui.input("a name no workspace has");
    ui.wait("the search to still have the keys", |ui| ui.has("home-nothing"));
}
