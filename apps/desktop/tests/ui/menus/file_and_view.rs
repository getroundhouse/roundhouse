//! The File and View menus, whose items are the window's own shortcuts. What each shortcut does is tested by
//! pressing it, beside what it's about, so here choosing one is only the example that shows the wiring.

use gpui_kit::{AppContext as _, TestAppContext};

use super::{action, choose, enabled, items, key, keyed, with_a_repository};
use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn the_file_menu_makes_things_and_closes_a_chat(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);

    let made = ["New Workspace", "New Chat", "Add Project…", "-", "Close Chat"];
    // Off the Mac, what a Mac has in its app menu ends it, as `menus::tests` checks for each system.
    assert_eq!(items(&ui, "File")[..made.len()], made);
    assert!(action(&ui, "File", "New Workspace").partial_eq(&desktop::NewWorkspace));
    assert!(action(&ui, "File", "New Chat").partial_eq(&desktop::NewChat));
    assert!(action(&ui, "File", "Add Project…").partial_eq(&desktop::AddRepository));
    assert!(action(&ui, "File", "Close Chat").partial_eq(&desktop::CloseChat));
    assert_eq!(key(&ui, "File", "New Workspace"), Some(keyed(&ui, "secondary-n")));
    assert_eq!(key(&ui, "File", "New Chat"), Some(keyed(&ui, "secondary-t")));
    assert_eq!(key(&ui, "File", "Add Project…"), None);
    assert_eq!(key(&ui, "File", "Close Chat"), Some(keyed(&ui, "secondary-w")));
}

#[gpui_kit::test]
fn the_view_menu_shows_the_diff_and_moves_between_workspaces(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);

    // macOS adds Enter Full Screen below the last separator, which only a Mac has.
    let full_screen: &[&str] = if cfg!(target_os = "macos") { &["-"] } else { &[] };
    assert_eq!(
        items(&ui, "View"),
        [&["Show Diff", "-", "Previous Workspace", "Next Workspace"][..], full_screen].concat()
    );
    assert!(action(&ui, "View", "Show Diff").partial_eq(&desktop::ToggleDiff));
    assert!(action(&ui, "View", "Previous Workspace").partial_eq(&desktop::PreviousWorkspace));
    assert!(action(&ui, "View", "Next Workspace").partial_eq(&desktop::NextWorkspace));
    assert_eq!(key(&ui, "View", "Show Diff"), Some(keyed(&ui, "secondary-shift-d")));
    assert_eq!(key(&ui, "View", "Previous Workspace"), Some(keyed(&ui, "secondary-[")));
    assert_eq!(key(&ui, "View", "Next Workspace"), Some(keyed(&ui, "secondary-]")));
}

/// As the header's button does, whichever way the diff opened or closed.
#[gpui_kit::test]
fn show_diff_is_hide_diff_while_the_diff_is_open(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);
    ui.new_workspace();
    assert_eq!(items(&ui, "View")[0], "Hide Diff", "it starts open");

    choose(&mut ui, "View", "Hide Diff");

    assert!(!ui.changes_open());
    assert_eq!(items(&ui, "View")[0], "Show Diff");
    ui.click("toggle-diff");
    assert_eq!(items(&ui, "View")[0], "Hide Diff");
}

/// ⌘W closes the file tab while it shows, rather than the chat behind it, so the item says what it'll close.
#[gpui_kit::test]
fn close_chat_is_close_file_tab_while_the_file_tab_shows(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);
    let workspace = ui.new_workspace();
    std::fs::write(ui.worktree(&workspace).join("a.txt"), "one\ntwo\n").unwrap();
    let diff = ui.open_changes();
    ui.open_diff_of(&diff, "a.txt");
    assert_eq!(items(&ui, "File")[4], "Close File Tab");
    assert_eq!(key(&ui, "File", "Close File Tab"), Some(keyed(&ui, "secondary-w")));

    choose(&mut ui, "File", "Close File Tab");

    assert!(ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.file_tab().is_none()));
    assert_eq!(items(&ui, "File")[4], "Close Chat");
}

/// macOS greys out an item when nothing where focus is takes its action, so the window takes each only where it can
/// act.
#[gpui_kit::test]
fn file_and_view_items_are_greyed_out_until_they_can_act(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    let needs_a_workspace =
        [("File", "New Chat"), ("File", "Close Chat"), ("View", "Show Diff"), ("View", "Next Workspace")];
    assert!(enabled(&mut ui, "File", "Add Project…"));
    assert!(!enabled(&mut ui, "File", "New Workspace"), "there's no repository to put it in");
    for (menu, item) in needs_a_workspace {
        assert!(!enabled(&mut ui, menu, item), "{item} with no workspace");
    }

    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    assert!(enabled(&mut ui, "File", "New Workspace"));
    assert!(!enabled(&mut ui, "File", "New Chat"));
    ui.new_workspace();

    for (menu, item) in needs_a_workspace {
        // The sidebar of changes starts open, so its item offers to hide it.
        let item = if item == "Show Diff" { "Hide Diff" } else { item };
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
