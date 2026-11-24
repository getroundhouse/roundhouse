//! The File and View menus, whose items are the window's own shortcuts. What each shortcut does is tested by
//! pressing it, beside what it's about, so here choosing one is only the example that shows the wiring. Which item
//! holds which action, and its key, is in `bindings.rs`.

use client_core::fake_hub::World;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::{CommitId, FileContents, GitDiff};

use super::{can_find, choose, enabled, in_a_workspace, items, key, keyed, on};

/// What needs a workspace to act on.
const NEEDS_A_WORKSPACE: [(&str, &str); 8] = [
    ("File", "New Chat"),
    ("File", "New Terminal"),
    ("File", "Close Chat"),
    ("File", "Archive Workspace"),
    ("View", "Show Diff"),
    ("View", "Next Workspace"),
    ("View", "Previous Tab"),
    ("View", "Next Tab"),
];

/// macOS greys out an item when nothing where focus is takes its action, so the window takes each only where it can
/// act: New Workspace once there's a repository to put it in, and the rest once there's a workspace. Nor can Paste or
/// Select All act where there's no text box, but Copy can, since text selected in a chat copies wherever focus is.
#[gpui_kit::test]
fn with_no_repository_add_project_can_be_chosen_and_new_workspace_cant(cx: &mut TestAppContext) {
    let (_hub, mut ui) = on(cx, World::default());

    assert!(enabled(&mut ui, "File", "Add Project…"), "Add Project… with nothing to put it in");
    assert!(!enabled(&mut ui, "File", "New Workspace"), "there's no repository to put it in");
    for (menu, item) in NEEDS_A_WORKSPACE {
        assert!(!enabled(&mut ui, menu, item), "{item} with no workspace");
    }
}

/// There's no chat to find in either.
#[gpui_kit::test]
fn with_a_repository_and_no_workspace_new_workspace_and_copy_can_be_chosen_and_the_rest_cant(cx: &mut TestAppContext) {
    let mut world = World::default();
    world.repo("signals");
    let (_hub, mut ui) = on(cx, world);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);

    assert!(enabled(&mut ui, "File", "New Workspace"), "with a repository");
    for (menu, item) in NEEDS_A_WORKSPACE {
        assert!(!enabled(&mut ui, menu, item), "{item} with a repository and no workspace");
    }
    assert!(!enabled(&mut ui, "Edit", "Paste") && !enabled(&mut ui, "Edit", "Select All"), "with no text box");
    assert!(enabled(&mut ui, "Edit", "Copy"), "Copy, wherever focus is");
    assert!(!can_find(&mut ui), "with no chat to find in");
}

/// The sidebar of changes starts open, so its item offers to hide it, and says what it'll do as the header's button
/// does, whichever way the diff opened or closed. ⌘W closes the file tab while it shows, rather than the chat behind
/// it, so the item says what it'll close.
#[gpui_kit::test]
fn in_a_workspace_every_item_can_be_chosen_and_says_what_it_does_to_what_shows(cx: &mut TestAppContext) {
    let (_hub, mut ui) = in_a_workspace(cx, |world, workspace| {
        let patch = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1,2 @@\n one\n+two\n";
        let base_commit = CommitId::from("3f4c19c8a2d1e0b7f6a5c4d3e2f1a0b9c8d7e6f5");
        world.diffs.insert(workspace.clone(), GitDiff { base_commit, patch: patch.to_string(), truncated: false });
        world.base_files.insert("a.txt".to_string(), FileContents::Text { text: "one\n".to_string() });
    });
    for (menu, item) in NEEDS_A_WORKSPACE {
        let item = if item == "Show Diff" { "Hide Diff" } else { item };
        assert!(enabled(&mut ui, menu, item), "{item} in a workspace");
    }
    assert!(enabled(&mut ui, "File", "New Workspace"), "New Workspace in a workspace");
    assert_eq!(items(&ui, "View")[1], "Hide Diff", "it starts open");

    choose(&mut ui, "View", "Hide Diff");

    assert!(!ui.changes_open(), "Hide Diff hid it");
    assert_eq!(items(&ui, "View")[1], "Show Diff", "hidden from the menu");
    ui.click("toggle-diff");
    assert_eq!(items(&ui, "View")[1], "Hide Diff", "shown from the header's button");

    let diff = ui.open_changes();
    ui.open_diff_of(&diff, "a.txt");
    assert_eq!(items(&ui, "File")[5], "Close File Tab", "with the file tab showing");
    assert_eq!(key(&ui, "File", "Close File Tab"), Some(keyed(&ui, "secondary-w")));

    choose(&mut ui, "File", "Close File Tab");

    assert!(ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.file_tab().is_none()), "Close File Tab closed it");
    assert_eq!(items(&ui, "File")[5], "Close Chat", "with the chat showing again");
}
