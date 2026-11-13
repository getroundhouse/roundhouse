//! The menus: what each holds, the key beside each item, and what the items do. Hiding, minimizing and zooming happen
//! in macOS, which a headless window doesn't have, so for those the tests stop at the action. On a Mac the menus are
//! the menu bar, and elsewhere the app draws them, behind the button `drawn.rs` tests.

#![cfg(test)]

use gpui_kit::component::input::{
    Copy, Cut, DeleteToPreviousWordStart, MoveToPreviousWord, Paste, Redo, SelectAll, Undo,
};
use gpui_kit::{Action, AppContext as _, Keystroke, OperatingSystem, OwnedMenu, OwnedMenuItem, TestAppContext};

use crate::support::{Stack, Ui};

#[cfg(not(target_os = "macos"))]
mod drawn;
mod file_and_view;

/// Where the items a Mac has in its app menu are on the system the tests run on, as the menu and the item: in the
/// app menu on a Mac, and elsewhere in File, or in Help.
pub(crate) const ABOUT: (&str, &str) =
    if ON_A_MAC { ("Roundhouse", "About Roundhouse") } else { ("Help", "About Roundhouse") };
pub(crate) const SETTINGS: (&str, &str) =
    if ON_A_MAC { ("Roundhouse", "Settings…") } else { ("File", "Settings…") };
pub(crate) const CHECK_FOR_UPDATES: (&str, &str) =
    if ON_A_MAC { ("Roundhouse", "Check for Updates…") } else { ("Help", "Check for Updates…") };
pub(crate) const QUIT: (&str, &str) = if ON_A_MAC {
    ("Roundhouse", "Quit Roundhouse")
} else if cfg!(target_os = "windows") {
    ("File", "Exit")
} else {
    ("File", "Quit")
};
const ON_A_MAC: bool = cfg!(target_os = "macos");

/// A menu from the menu bar, by the name it's given.
fn menu(ui: &Ui<'_>, name: &str) -> OwnedMenu {
    let menus = ui.cx.update(|cx| cx.get_menus()).unwrap();
    menus.into_iter().find(|menu| menu.name == name).unwrap_or_else(|| panic!("no {name} menu"))
}

/// A menu's items as macOS lists them, with `-` for each separator.
pub(crate) fn items(ui: &Ui<'_>, name: &str) -> Vec<String> {
    let items = menu(ui, name).items.into_iter();
    items
        .map(|item| match item {
            OwnedMenuItem::Separator => "-".to_string(),
            OwnedMenuItem::Action { name, .. } => name,
            OwnedMenuItem::Submenu(menu) => menu.name.to_string(),
            OwnedMenuItem::SystemMenu(menu) => menu.name.to_string(),
        })
        .collect()
}

/// What choosing an item does.
fn action(ui: &Ui<'_>, menu_name: &str, name: &str) -> Box<dyn Action> {
    let action = menu(ui, menu_name).items.into_iter().find_map(|item| match item {
        OwnedMenuItem::Action { name: shown, action, .. } if shown == name => Some(action),
        _ => None,
    });
    action.unwrap_or_else(|| panic!("no {name} in the {menu_name} menu"))
}

/// The key shown beside an item, as [`keyed`] writes it.
fn key(ui: &Ui<'_>, menu_name: &str, name: &str) -> Option<String> {
    key_for(ui, action(ui, menu_name, name).as_ref())
}

/// The key shown beside an item that does `action`: on a Mac the first bound to it, which macOS shows, and elsewhere
/// the last, which the kit shows.
fn key_for(ui: &Ui<'_>, action: &dyn Action) -> Option<String> {
    let os = ui.cx.update(|cx| cx.platform_info().os);
    let keymap = ui.cx.update(|cx| cx.key_bindings());
    let keymap = keymap.borrow();
    let mut bindings = keymap.bindings_for_action(action);
    let binding = if os == OperatingSystem::Mac { bindings.next() } else { bindings.next_back() }?;
    let keystrokes = binding.keystrokes().iter().map(|keystroke| keystroke.unparse(os));
    Some(keystrokes.collect::<Vec<_>>().join(" "))
}

/// `keys`, written as they're bound, as [`key`] gives them on the system the tests run on, so `secondary-n` is `cmd-n`
/// on a Mac and `ctrl-n` elsewhere.
fn keyed(ui: &Ui<'_>, keys: &str) -> String {
    let os = ui.cx.update(|cx| cx.platform_info().os);
    Keystroke::parse_with_os(keys, os).unwrap().unparse(os)
}

/// What pressing a key would do where focus is, without doing it, or `None` for nothing.
fn bound_where_focus_is(ui: &mut Ui<'_>, key: &str) -> Option<Box<dyn Action>> {
    ui.frame();
    let os = ui.cx.update(|cx| cx.platform_info().os);
    let keystroke = Keystroke::parse_with_os(key, os).unwrap();
    let keymap = ui.cx.update(|cx| cx.key_bindings());
    let contexts = ui.cx.update_window(ui.handle, |_, window, _| window.context_stack()).unwrap();
    let (bindings, _) = keymap.borrow().bindings_for_input(&[keystroke], &contexts);
    bindings.first().map(|binding| binding.action().boxed_clone())
}

/// Whether macOS shows the item as one that can be chosen, which is whether anything where focus is takes its action.
fn enabled(ui: &mut Ui<'_>, menu_name: &str, name: &str) -> bool {
    let action = action(ui, menu_name, name);
    ui.frame();
    ui.cx.update(|cx| cx.is_action_available(action.as_ref()))
}

/// Chooses an item, as the menu bar does.
pub(crate) fn choose(ui: &mut Ui<'_>, menu_name: &str, name: &str) {
    let action = action(ui, menu_name, name);
    ui.cx.update(|cx| cx.dispatch_action(action.as_ref()));
    ui.frame();
}

/// Opens the app on a repository with no workspace, so there's no composer.
fn with_a_repository<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui
}

/// Opens a workspace, so its composer has focus, with this typed in it.
fn typing<'a>(cx: &'a mut TestAppContext, stack: &Stack, typed: &str) -> Ui<'a> {
    let mut ui = with_a_repository(cx, stack);
    ui.new_workspace();
    ui.input(typed);
    ui
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn the_app_menu_has_what_every_mac_app_has_in_the_same_order(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);

    assert_eq!(
        items(&ui, "Roundhouse"),
        [
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
        ]
    );
    assert!(action(&ui, "Roundhouse", "Hide Roundhouse").partial_eq(&desktop::Hide));
    assert!(action(&ui, "Roundhouse", "Hide Others").partial_eq(&desktop::HideOthers));
    assert!(action(&ui, "Roundhouse", "Show All").partial_eq(&desktop::ShowAll));
    assert!(action(&ui, "Roundhouse", "Quit Roundhouse").partial_eq(&desktop::Quit));
    assert!(action(&ui, "Roundhouse", "Settings…").partial_eq(&desktop::OpenSettings));
    assert_eq!(key(&ui, "Roundhouse", "Hide Roundhouse"), Some(keyed(&ui, "secondary-h")));
    assert_eq!(key(&ui, "Roundhouse", "Hide Others"), Some(keyed(&ui, "alt-secondary-h")));
    assert_eq!(key(&ui, "Roundhouse", "Show All"), None);
    assert_eq!(key(&ui, "Roundhouse", "Quit Roundhouse"), Some(keyed(&ui, "secondary-q")));
    assert_eq!(key(&ui, "Roundhouse", "Settings…"), Some(keyed(&ui, "secondary-,")));
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn the_window_menu_has_minimize_and_zoom(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);

    // macOS puts its own items above these.
    assert_eq!(items(&ui, "Window"), ["-", "Minimize", "Zoom"]);
    assert!(action(&ui, "Window", "Minimize").partial_eq(&desktop::Minimize));
    assert!(action(&ui, "Window", "Zoom").partial_eq(&desktop::Zoom));
    assert_eq!(key(&ui, "Window", "Minimize"), Some(keyed(&ui, "secondary-m")));
    assert_eq!(key(&ui, "Window", "Zoom"), None);
}

/// A text box binds keys of its own, and none of them may take these. Off the Mac, Cmd is the Windows or Super key,
/// which the system keeps, so what only a Mac does has no key there, and the others are on Ctrl.
#[gpui_kit::test]
fn the_app_and_window_keys_work_while_typing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing(cx, &stack, "say:not sent yet");

    let keys: [(&str, &dyn Action, bool); 4] = [
        ("secondary-h", &desktop::Hide, ON_A_MAC),
        ("alt-secondary-h", &desktop::HideOthers, ON_A_MAC),
        ("secondary-q", &desktop::Quit, true),
        ("secondary-m", &desktop::Minimize, ON_A_MAC),
    ];
    for (key, item, bound) in keys {
        let pressed = bound_where_focus_is(&mut ui, key);
        assert_eq!(pressed.is_some_and(|pressed| pressed.partial_eq(item)), bound, "{key} and {}", item.name());
    }
    if !ON_A_MAC {
        assert!(bound_where_focus_is(&mut ui, "cmd-q").is_none(), "the Windows or Super key quits nothing");
    }
}

/// Off the Mac the text box's editing keys are on Ctrl, as the app's are, and none of the app's may take them.
#[gpui_kit::test]
fn the_text_box_keeps_its_own_keys(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing(cx, &stack, "say:not sent yet");

    let mut edits: Vec<(&str, &dyn Action)> = vec![
        ("secondary-a", &SelectAll),
        ("secondary-c", &Copy),
        ("secondary-x", &Cut),
        ("secondary-v", &Paste),
        ("secondary-z", &Undo),
    ];
    // Keys only the text box off the Mac has, which an app-wide key could take there without a Mac's tests seeing.
    if !ON_A_MAC {
        edits.extend([
            ("ctrl-y", &Redo as &dyn Action),
            ("ctrl-left", &MoveToPreviousWord),
            ("ctrl-backspace", &DeleteToPreviousWordStart),
        ]);
    }
    for (key, edit) in edits {
        let pressed = bound_where_focus_is(&mut ui, key);
        assert!(pressed.is_some_and(|pressed| pressed.partial_eq(edit)), "{key} isn't the text box's");
    }
}

/// The kit's own actions, so each item does what its key does in a text box.
#[gpui_kit::test]
fn the_edit_menu_has_the_text_box_keys(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);

    assert_eq!(items(&ui, "Edit"), ["Undo", "Redo", "-", "Cut", "Copy", "Paste", "Select All", "-", "Find"]);
    let kit: [(&str, &dyn Action, &str); 6] = [
        ("Undo", &Undo, "cmd-z"),
        ("Redo", &Redo, "cmd-shift-z"),
        ("Cut", &Cut, "cmd-x"),
        ("Copy", &Copy, "cmd-c"),
        ("Paste", &Paste, "cmd-v"),
        ("Select All", &SelectAll, "cmd-a"),
    ];
    for (name, kits, shown) in kit {
        assert!(action(&ui, "Edit", name).partial_eq(kits), "{name} isn't the kit's");
        // Elsewhere the kit binds other keys, and there's no menu bar to show them.
        if ON_A_MAC {
            assert_eq!(key(&ui, "Edit", name).as_deref(), Some(shown), "{name}");
        }
    }
}

/// Find has a menu of its own under the text box's items, as a Mac app has it, and finds in the chat showing. What it
/// does is tested by pressing its keys, in `finding.rs`.
#[gpui_kit::test]
fn the_edit_menu_has_find_for_the_chat_showing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);
    let Some(OwnedMenuItem::Submenu(find)) = menu(&ui, "Edit").items.into_iter().last() else {
        panic!("Find isn't a menu at the end of Edit")
    };
    let shown: Vec<(String, Box<dyn Action>)> = find
        .items
        .into_iter()
        .filter_map(|item| match item {
            OwnedMenuItem::Action { name, action, .. } => Some((name, action)),
            _ => None,
        })
        .collect();
    let wanted: [(&str, &dyn Action, &str); 3] = [
        ("Find…", &desktop::FindInChat, "secondary-f"),
        ("Find Next", &desktop::FindNext, "secondary-g"),
        ("Find Previous", &desktop::FindPrevious, "secondary-shift-g"),
    ];
    assert_eq!(shown.len(), wanted.len());
    for ((name, action), (wanted_name, wanted_action, wanted_key)) in shown.iter().zip(wanted) {
        assert_eq!(name, wanted_name);
        assert!(action.partial_eq(wanted_action), "{name}");
        assert_eq!(key_for(&ui, action.as_ref()), Some(keyed(&ui, wanted_key)), "{name}");
    }
    let available = |ui: &mut Ui<'_>| {
        ui.frame();
        ui.cx.update(|cx| cx.is_action_available(&desktop::FindInChat))
    };
    assert!(!available(&mut ui), "with no chat to find in");

    ui.new_workspace();

    assert!(available(&mut ui));
    let (pane, transcript) = (ui.pane().unwrap(), ui.transcript().unwrap());
    let chat = ui.cx.read_entity(&transcript, |t, _| t.session_id().clone());
    ui.cx.update(|cx| pane.update(cx, |p, cx| p.open_subagent_tab(chat, "call-1".into(), "Look around".into(), cx)));
    assert!(!available(&mut ui), "with the subagent tab in front, which isn't searched");
}

/// Choosing the items, rather than pressing their keys, which the kit's own tests cover.
#[gpui_kit::test]
fn the_edit_menu_edits_the_composer(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = typing(cx, &stack, "say:hello");

    choose(&mut ui, "Edit", "Select All");
    choose(&mut ui, "Edit", "Cut");
    assert_eq!(ui.composer_text(), "");
    choose(&mut ui, "Edit", "Paste");
    choose(&mut ui, "Edit", "Paste");
    assert_eq!(ui.composer_text(), "say:hellosay:hello");
    choose(&mut ui, "Edit", "Undo");

    assert_eq!(ui.composer_text(), "say:hello");
}

/// macOS greys out an item when nothing where focus is would take its action.
#[gpui_kit::test]
fn the_edit_menu_is_greyed_out_where_theres_no_text_box(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);
    assert!(!enabled(&mut ui, "Edit", "Paste"));
    assert!(!enabled(&mut ui, "Edit", "Select All"));
    // Text selected in a chat copies, wherever focus is.
    assert!(enabled(&mut ui, "Edit", "Copy"));

    ui.new_workspace();

    assert!(enabled(&mut ui, "Edit", "Paste"));
    assert!(enabled(&mut ui, "Edit", "Select All"));
}

#[gpui_kit::test]
fn about_says_which_build_this_is(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);

    choose(&mut ui, ABOUT.0, ABOUT.1);

    ui.wait("the alert", |ui| ui.cx.has_pending_prompt());
    let (message, detail) = ui.cx.pending_prompt().unwrap();
    assert_eq!(message, "Roundhouse");
    assert_eq!(detail, format!("Version {}", stack.hub.build_version));
}

/// A Mac's, in its menu bar, and elsewhere as Windows arranges menus, which the app draws. What each holds on every
/// system is tested beside where they're made, in `menus::tests`.
#[gpui_kit::test]
fn the_menus_are_the_ones_this_system_has(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);

    let menus = ui.cx.update(|cx| cx.get_menus()).unwrap();

    let names: Vec<String> = menus.iter().map(|menu| menu.name.to_string()).collect();
    let wanted: &[&str] =
        if ON_A_MAC { &["Roundhouse", "File", "Edit", "View", "Window"] } else { &["File", "Edit", "View", "Help"] };
    assert_eq!(names, wanted);
}
