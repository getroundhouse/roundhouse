//! The File and View menus, whose items are the window's own shortcuts. What each shortcut does is tested by
//! pressing it, beside what it's about, so here choosing one is only the example that shows the wiring.

use gpui_kit::{AppContext as _, TestAppContext};

use super::{action, choose, enabled, items, key, with_a_repository};
use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn the_file_menu_makes_things_and_closes_a_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);

    assert_eq!(items(&ui, "File"), ["New Workspace", "New Chat", "Add Repository…", "-", "Close Chat"]);
    assert!(action(&ui, "File", "New Workspace").partial_eq(&desktop::NewWorkspace));
    assert!(action(&ui, "File", "New Chat").partial_eq(&desktop::NewChat));
    assert!(action(&ui, "File", "Add Repository…").partial_eq(&desktop::AddRepository));
    assert!(action(&ui, "File", "Close Chat").partial_eq(&desktop::CloseChat));
    assert_eq!(key(&ui, "File", "New Workspace").as_deref(), Some("cmd-n"));
    assert_eq!(key(&ui, "File", "New Chat").as_deref(), Some("cmd-t"));
    assert_eq!(key(&ui, "File", "Add Repository…"), None);
    assert_eq!(key(&ui, "File", "Close Chat").as_deref(), Some("cmd-w"));
}

#[gpui_kit::test]
fn the_view_menu_shows_the_diff_and_moves_between_workspaces(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);

    // macOS adds Enter Full Screen below the last separator.
    assert_eq!(items(&ui, "View"), ["Show Diff", "-", "Previous Workspace", "Next Workspace", "-"]);
    assert!(action(&ui, "View", "Show Diff").partial_eq(&desktop::ToggleDiff));
    assert!(action(&ui, "View", "Previous Workspace").partial_eq(&desktop::PreviousWorkspace));
    assert!(action(&ui, "View", "Next Workspace").partial_eq(&desktop::NextWorkspace));
    assert_eq!(key(&ui, "View", "Show Diff").as_deref(), Some("cmd-shift-d"));
    assert_eq!(key(&ui, "View", "Previous Workspace").as_deref(), Some("cmd-["));
    assert_eq!(key(&ui, "View", "Next Workspace").as_deref(), Some("cmd-]"));
}

/// As the header's button does, whichever way the diff opened or closed.
#[gpui_kit::test]
fn show_diff_is_hide_diff_while_the_diff_is_open(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);
    ui.new_workspace();
    assert_eq!(items(&ui, "View")[0], "Show Diff");

    choose(&mut ui, "View", "Show Diff");

    assert!(ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.diff_open));
    assert_eq!(items(&ui, "View")[0], "Hide Diff");
    ui.click("toggle-diff");
    assert_eq!(items(&ui, "View")[0], "Show Diff");
}

/// macOS greys out an item when nothing where focus is takes its action, so the window takes each only where it can
/// act.
#[gpui_kit::test]
fn file_and_view_items_are_greyed_out_until_they_can_act(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    let needs_a_workspace =
        [("File", "New Chat"), ("File", "Close Chat"), ("View", "Show Diff"), ("View", "Next Workspace")];
    assert!(enabled(&mut ui, "File", "Add Repository…"));
    assert!(!enabled(&mut ui, "File", "New Workspace"), "there's no repository to put it in");
    for (menu, item) in needs_a_workspace {
        assert!(!enabled(&mut ui, menu, item), "{item} with no workspace");
    }

    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    assert!(enabled(&mut ui, "File", "New Workspace"));
    assert!(!enabled(&mut ui, "File", "New Chat"));
    ui.new_workspace();

    for (menu, item) in needs_a_workspace {
        assert!(enabled(&mut ui, menu, item), "{item} in a workspace");
    }
}

#[gpui_kit::test]
fn choosing_new_chat_opens_one(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);
    ui.new_workspace();
    let before = ui.open_chats().len();

    choose(&mut ui, "File", "New Chat");

    ui.wait("the new chat", |ui| ui.open_chats().len() == before + 1);
}
