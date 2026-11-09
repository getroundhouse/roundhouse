//! The menu bar, and what its items do that no view does. Each item is an action, so macOS shows its key beside it.

use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
use gpui_kit::{App, Menu, MenuItem, OsAction, PromptLevel, SystemMenuType, Window};

use crate::{
    About, AddRepository, CheckForUpdates, CloseChat, Hide, HideOthers, Minimize, NewChat, NewWorkspace, NextWorkspace,
    OpenSettings, PreviousWorkspace, Quit, ShowAll, ToggleDiff, Zoom,
};

/// What the menu bar says about the selected workspace. macOS shows only what it was last given, so the window gives
/// it the bar again when this changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Says {
    /// The diff is open, so View has Hide Diff rather than Show Diff.
    pub(crate) diff_open: bool,
}

/// Sets the menu bar, and handles the items no view does, wherever focus is. The window shows Settings itself. macOS
/// reads each item's key from the bindings each time the bar is given, so they're bound first.
pub(crate) fn set(cx: &mut App) {
    show(Says::default(), cx);
    cx.on_action(|_: &About, cx| about(cx));
    cx.on_action(|_: &CheckForUpdates, cx| crate::updates::check_now(cx));
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Minimize, cx| in_front(cx, Window::minimize_window));
    cx.on_action(|_: &Zoom, cx| in_front(cx, Window::zoom_window));
}

/// Gives macOS the menu bar, saying this.
pub(crate) fn show(says: Says, cx: &mut App) {
    cx.set_menus([app_menu(), file_menu(), edit_menu(), view_menu(says), window_menu()]);
}

/// The items every Mac app has, in the same order and groups, with the app's own after About.
fn app_menu() -> Menu {
    // macOS names the first menu after the app, whatever it's called here.
    Menu::new("Roundhouse").items([
        MenuItem::action("About Roundhouse", About),
        MenuItem::separator(),
        MenuItem::action("Check for Updates…", CheckForUpdates),
        MenuItem::separator(),
        MenuItem::action("Settings…", OpenSettings),
        MenuItem::separator(),
        MenuItem::os_submenu("Services", SystemMenuType::Services),
        MenuItem::separator(),
        MenuItem::action("Hide Roundhouse", Hide),
        MenuItem::action("Hide Others", HideOthers),
        MenuItem::action("Show All", ShowAll),
        MenuItem::separator(),
        MenuItem::action("Quit Roundhouse", Quit),
    ])
}

/// Making things, and closing a chat, as a File menu makes and closes documents. The window handles these.
fn file_menu() -> Menu {
    Menu::new("File").items([
        MenuItem::action("New Workspace", NewWorkspace),
        MenuItem::action("New Chat", NewChat),
        MenuItem::action("Add Repository…", AddRepository),
        MenuItem::separator(),
        MenuItem::action("Close Chat", CloseChat),
    ])
}

/// The kit's text boxes handle these, so each works on whichever has focus, and is greyed out where none does. Each is
/// also the system's own, which macOS sends to its own text fields first. macOS adds its own items at the bottom,
/// such as Emoji & Symbols.
fn edit_menu() -> Menu {
    Menu::new("Edit").items([
        MenuItem::os_action("Undo", Undo, OsAction::Undo),
        MenuItem::os_action("Redo", Redo, OsAction::Redo),
        MenuItem::separator(),
        MenuItem::os_action("Cut", Cut, OsAction::Cut),
        MenuItem::os_action("Copy", Copy, OsAction::Copy),
        MenuItem::os_action("Paste", Paste, OsAction::Paste),
        MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
    ])
}

/// What's on screen: the diff, and which workspace. The window handles these. macOS adds Enter Full Screen at the
/// bottom, after the separator that keeps it apart. Moving between chats isn't here, because GPUI can't give macOS
/// Tab as an item's key, so ⌃⇥ would show as ⌃T.
fn view_menu(says: Says) -> Menu {
    let diff = if says.diff_open { "Hide Diff" } else { "Show Diff" };
    Menu::new("View").items([
        MenuItem::action(diff, ToggleDiff),
        MenuItem::separator(),
        MenuItem::action("Previous Workspace", PreviousWorkspace),
        MenuItem::action("Next Workspace", NextWorkspace),
        MenuItem::separator(),
    ])
}

/// Named Window, so GPUI makes it the system's window menu, and macOS adds the rest: arranging the window, full
/// screen, and the list of windows. macOS puts its items above these, since it doesn't know them as its own
/// Minimize and Zoom, so a separator keeps them a group.
fn window_menu() -> Menu {
    Menu::new("Window").items([
        MenuItem::separator(),
        MenuItem::action("Minimize", Minimize),
        MenuItem::action("Zoom", Zoom),
    ])
}

/// Says which build this is, the same as the sidebar does.
fn about(cx: &mut App) {
    let version = format!("Version {}", protocol::BUILD_VERSION);
    crate::alert::show(PromptLevel::Info, "Roundhouse".to_string(), version, cx);
}

/// Does something to the window in front, once the action that asked for it is done. GPUI lends a window out while
/// an action goes through it, and until it's back the window can't be reached. With every window minimized there's
/// none in front, so nothing happens.
fn in_front(cx: &mut App, act: fn(&Window)) {
    cx.defer(move |cx| {
        if let Some(window) = cx.active_window() {
            // Fails only for a window that has closed since, which has nothing left to do it to.
            window.update(cx, |_, window, _| act(window)).ok();
        }
    });
}
