//! What the menu bar should hold on the system the tests run on: each menu's items, and each item's action and key,
//! which `bindings.rs` checks the window's menus against.

use gpui_kit::Action;

use super::bindings::ON_A_MAC;

/// Each menu this system has, in order, with its items, `-` for each separator. Off the Mac, what a Mac has in its app
/// menu ends File, as `menus::tests` checks for each system, so File's are only those it starts with.
/// macOS adds Enter Full Screen below View's last separator, which only a Mac has, and puts its own items above
/// Window's. The window has no workspace, so View offers to show the diff, and to hide the sidebar, which shows.
pub(super) fn menus_and_their_items() -> Vec<(&'static str, Vec<&'static str>)> {
    let file = vec![
        "New Workspace",
        "New Chat",
        "New Terminal",
        "Add Project…",
        "-",
        "Close Chat",
        "Mark as Unread",
        "Archive Workspace",
    ];
    let edit = vec!["Undo", "Redo", "-", "Cut", "Copy", "Paste", "Select All", "-", "Find"];
    let mut view = vec![
        "Hide Sidebar",
        "Show Diff",
        "-",
        "Back",
        "Forward",
        "Previous Workspace",
        "Next Workspace",
        "-",
        "Previous Tab",
        "Next Tab",
    ];
    if !ON_A_MAC {
        return vec![
            ("File", file),
            ("Edit", edit),
            ("View", view),
            ("Help", vec!["Check for Updates…", "-", "About Roundhouse"]),
        ];
    }
    view.push("-");
    let app_menu = vec![
        "About Roundhouse",
        "Check for Updates…",
        "-",
        "Settings…",
        "Services",
        "-",
        "Hide Roundhouse",
        "Hide Others",
        "Show All",
        "-",
        "Quit Roundhouse",
    ];
    vec![
        ("Roundhouse", app_menu),
        ("File", file),
        ("Edit", edit),
        ("View", view),
        ("Window", vec!["-", "Minimize", "Zoom"]),
    ]
}

/// Each item of the window's own, by its menu and its name, with what it does and the key it shows, as [`keyed`]
/// writes it, or `None` for one with no key.
pub(super) fn items_with_their_actions_and_keys()
-> Vec<(&'static str, &'static str, &'static dyn Action, Option<&'static str>)> {
    let mut rows: Vec<(&str, &str, &dyn Action, Option<&str>)> = vec![
        ("File", "New Workspace", &desktop::NewWorkspace, Some("secondary-n")),
        ("File", "New Chat", &desktop::NewChat, Some("secondary-t")),
        ("File", "New Terminal", &desktop::NewTerminal, Some("secondary-shift-t")),
        ("File", "Add Project…", &desktop::AddRepository, None),
        ("File", "Close Chat", &desktop::CloseChat, Some("secondary-w")),
        ("File", "Mark as Unread", &desktop::MarkWorkspace, Some("secondary-shift-u")),
        ("File", "Archive Workspace", &desktop::ArchiveWorkspace, Some("secondary-shift-a")),
        ("View", "Hide Sidebar", &desktop::ToggleSidebar, Some("secondary-b")),
        ("View", "Show Diff", &desktop::ToggleDiff, Some("secondary-shift-b")),
        ("View", "Back", &desktop::GoBack, Some("secondary-[")),
        ("View", "Forward", &desktop::GoForward, Some("secondary-]")),
        ("View", "Previous Workspace", &desktop::PreviousWorkspace, Some("alt-secondary-up")),
        ("View", "Next Workspace", &desktop::NextWorkspace, Some("alt-secondary-down")),
        ("View", "Previous Tab", &desktop::PreviousChat, Some(if ON_A_MAC { "secondary-{" } else { "ctrl-shift-tab" })),
        ("View", "Next Tab", &desktop::NextChat, Some(if ON_A_MAC { "secondary-}" } else { "ctrl-tab" })),
    ];
    if ON_A_MAC {
        rows.extend([
            ("Roundhouse", "Hide Roundhouse", &desktop::Hide as &dyn Action, Some("secondary-h")),
            ("Roundhouse", "Hide Others", &desktop::HideOthers, Some("alt-secondary-h")),
            ("Roundhouse", "Show All", &desktop::ShowAll, None),
            ("Roundhouse", "Quit Roundhouse", &desktop::Quit, Some("secondary-q")),
            ("Roundhouse", "Settings…", &desktop::OpenSettings, Some("secondary-,")),
            ("Window", "Minimize", &desktop::Minimize, Some("secondary-m")),
            ("Window", "Zoom", &desktop::Zoom, None),
        ]);
    }
    rows
}
