use crate::workspace_pane::Showing;
use crate::{
    About, AddRepository, CheckForUpdates, CloseChat, FindInChat, FindNext, FindPrevious, Hide, HideOthers, Minimize,
    NewChat, NewWorkspace, NextWorkspace, OpenSettings, PreviousWorkspace, Quit, ShowAll, ToggleDiff, Zoom,
};
use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
use gpui_kit::{App, Menu, MenuItem, OsAction, PromptLevel, SystemMenuType, Window};

/// The system whose menus the app has, which is the one it's built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum System {
    Mac,
    Windows,
    Linux,
}

impl System {
    pub(crate) const THIS: Self = if cfg!(target_os = "macos") {
        Self::Mac
    } else if cfg!(target_os = "windows") {
        Self::Windows
    } else {
        Self::Linux
    };

    /// Whether the app draws the menus itself, behind a button in the window's top left corner, as it does wherever
    /// the system has no menu bar for them.
    pub(crate) const fn draws_the_menus(self) -> bool {
        match self {
            System::Mac => false,
            System::Windows | System::Linux => true,
        }
    }
}

/// What the menus say about the selected workspace. macOS shows only what it was last given, and the menus the app
/// draws are drawn from what it last gave GPUI, so the window gives them again when this changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Says {
    /// The sidebar of changes is open, so View has Hide Diff rather than Show Diff.
    pub(crate) diff_open: bool,
    /// What's showing under the tabs, which ⌘W closes, rather than the chat behind it when it's a tab.
    pub(crate) showing: Showing,
}

/// Sets the menus, and handles the items no view does, wherever focus is. The window shows Settings itself. macOS
/// reads each item's key from the bindings each time the bar is given, so they're bound first. `build_version` is the
/// app's, which About says.
pub(crate) fn set(build_version: &'static str, cx: &mut App) {
    show(Says::default(), cx);
    cx.on_action(move |_: &About, cx| about(build_version, cx));
    cx.on_action(|_: &CheckForUpdates, cx| crate::updates::check_now(cx));
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &Quit, cx| crate::app::quit(cx));
    cx.on_action(|_: &Minimize, cx| in_front(cx, Window::minimize_window));
    cx.on_action(|_: &Zoom, cx| in_front(cx, Window::zoom_window));
}

/// Gives the system the menus, saying this.
pub(crate) fn show(says: Says, cx: &mut App) {
    cx.set_menus(menus(System::THIS, says));
}

/// The menus `system` has. A Mac has its app menu and its Window menu. Elsewhere there's no app menu: its Settings…
/// and Quit end File, and About and Check for Updates… are in Help, while hiding apps and the Window menu are a Mac's
/// alone, and the window's own buttons minimize and maximize it.
pub(super) fn menus(system: System, says: Says) -> Vec<Menu> {
    match system {
        System::Mac => vec![app_menu(), file_menu(system, says), edit_menu(), view_menu(system, says), window_menu()],
        System::Windows | System::Linux => {
            vec![file_menu(system, says), edit_menu(), view_menu(system, says), help_menu()]
        }
    }
}

/// The items every Mac app has, in the same order, with the app's own after About. Check for Updates… goes with About,
/// and Services with Settings…, so the menu has fewer lines across it.
fn app_menu() -> Menu {
    // macOS names the first menu after the app, whatever it's called here.
    Menu::new("Roundhouse").items([
        MenuItem::action("About Roundhouse", About),
        MenuItem::action("Check for Updates…", CheckForUpdates),
        MenuItem::separator(),
        MenuItem::action("Settings…", OpenSettings),
        MenuItem::os_submenu("Services", SystemMenuType::Services),
        MenuItem::separator(),
        MenuItem::action("Hide Roundhouse", Hide),
        MenuItem::action("Hide Others", HideOthers),
        MenuItem::action("Show All", ShowAll),
        MenuItem::separator(),
        MenuItem::action(quits(System::Mac), Quit),
    ])
}

/// Making things, and closing a chat, as a File menu makes and closes documents. The window handles these. ⌘W closes
/// the file tab or the subagent tab while it shows, so the item says so then. Off the Mac, Settings… and quitting end
/// it, as they do a Windows or Linux app's File menu.
fn file_menu(system: System, says: Says) -> Menu {
    let close = match says.showing {
        Showing::Chat => "Close Chat",
        Showing::File => "Close File Tab",
        Showing::Subagent => "Close Subagent Tab",
    };
    let mut items = vec![
        MenuItem::action("New Workspace", NewWorkspace),
        MenuItem::action("New Chat", NewChat),
        MenuItem::action("Add Project…", AddRepository),
        MenuItem::separator(),
        MenuItem::action(close, CloseChat),
    ];
    match system {
        System::Mac => {}
        System::Windows | System::Linux => items.extend([
            MenuItem::separator(),
            MenuItem::action("Settings…", OpenSettings),
            MenuItem::separator(),
            MenuItem::action(quits(system), Quit),
        ]),
    }
    Menu::new("File").items(items)
}

/// What the item that quits is called: Exit on Windows, as its apps say, and Quit on Linux, as GNOME's and KDE's do.
fn quits(system: System) -> &'static str {
    match system {
        System::Mac => "Quit Roundhouse",
        System::Windows => "Exit",
        System::Linux => "Quit",
    }
}

/// The kit's text boxes handle the first of these, so each works on whichever has focus, and is greyed out where none
/// does. Each is also the system's own, which macOS sends to its own text fields first. Find is the window's, for the
/// chat showing, in a menu of its own as a Mac app has it. macOS adds its own items at the bottom, such as Emoji &
/// Symbols.
fn edit_menu() -> Menu {
    Menu::new("Edit").items([
        MenuItem::os_action("Undo", Undo, OsAction::Undo),
        MenuItem::os_action("Redo", Redo, OsAction::Redo),
        MenuItem::separator(),
        MenuItem::os_action("Cut", Cut, OsAction::Cut),
        MenuItem::os_action("Copy", Copy, OsAction::Copy),
        MenuItem::os_action("Paste", Paste, OsAction::Paste),
        MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
        MenuItem::separator(),
        MenuItem::submenu(Menu::new("Find").items([
            MenuItem::action("Find…", FindInChat),
            MenuItem::action("Find Next", FindNext),
            MenuItem::action("Find Previous", FindPrevious),
        ])),
    ])
}

/// What's on screen: the diff, and which workspace. The window handles these. On a Mac, macOS adds Enter Full Screen
/// at the bottom, after the separator that keeps it apart. Moving between chats isn't here, because GPUI can't give
/// macOS Tab as an item's key, so ⌃⇥ would show as ⌃T.
fn view_menu(system: System, says: Says) -> Menu {
    let diff = if says.diff_open { "Hide Diff" } else { "Show Diff" };
    let mut items = vec![
        MenuItem::action(diff, ToggleDiff),
        MenuItem::separator(),
        MenuItem::action("Previous Workspace", PreviousWorkspace),
        MenuItem::action("Next Workspace", NextWorkspace),
    ];
    match system {
        System::Mac => items.push(MenuItem::separator()),
        System::Windows | System::Linux => {}
    }
    Menu::new("View").items(items)
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

/// What a Mac has at the top of its app menu, at the end of the menus, as Windows and Linux apps have it, with About
/// last, as theirs have it.
fn help_menu() -> Menu {
    Menu::new("Help").items([
        MenuItem::action("Check for Updates…", CheckForUpdates),
        MenuItem::separator(),
        MenuItem::action("About Roundhouse", About),
    ])
}

/// Says which build this is, the same as the sidebar does.
fn about(build_version: &str, cx: &mut App) {
    let version = format!("Version {build_version}");
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
